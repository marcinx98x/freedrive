package com.freedrive.mobile

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.util.Base64
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.ReadableMap
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec
import kotlin.math.max

class DownloadsModule(
  reactContext: ReactApplicationContext
) : ReactContextBaseJavaModule(reactContext) {

  private val ioExecutor = Executors.newFixedThreadPool(2)

  override fun getName() = "FreeDriveDownloads"

  @ReactMethod
  fun beginDownload(fileName: String, promise: Promise) {
    try {
      ensureChannels()
      val id = nextNotificationId.getAndIncrement()
      val safeName = sanitizeFileName(fileName)
      val notification = NotificationCompat.Builder(reactApplicationContext, CHANNEL_PROGRESS)
        .setContentTitle("Downloading")
        .setContentText(safeName)
        .setSmallIcon(android.R.drawable.stat_sys_download)
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setProgress(0, 0, true)
        .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        .build()
      NotificationManagerCompat.from(reactApplicationContext).notify(id, notification)
      promise.resolve(id)
    } catch (error: Exception) {
      promise.reject("DOWNLOAD_NOTIFY_FAILED", error.message ?: "Could not show download status", error)
    }
  }

  @ReactMethod
  fun completeDownload(
    notificationId: Double,
    fileName: String,
    mimeType: String,
    contentUri: String,
    promise: Promise
  ) {
    try {
      ensureChannels()
      val id = notificationId.toInt()
      val safeName = sanitizeFileName(fileName)
      val mime = mimeType.ifBlank { "application/octet-stream" }
      val uri = Uri.parse(contentUri)

      val viewIntent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(uri, mime)
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      val chooser = Intent.createChooser(viewIntent, safeName).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      }
      val pendingFlags = PendingIntent.FLAG_UPDATE_CURRENT or
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) PendingIntent.FLAG_IMMUTABLE else 0
      val contentIntent = PendingIntent.getActivity(
        reactApplicationContext,
        id,
        chooser,
        pendingFlags
      )

      val notification = NotificationCompat.Builder(reactApplicationContext, CHANNEL_COMPLETE)
        .setContentTitle("Download complete")
        .setContentText(safeName)
        .setSmallIcon(android.R.drawable.stat_sys_download_done)
        .setOngoing(false)
        .setAutoCancel(true)
        .setContentIntent(contentIntent)
        .setCategory(NotificationCompat.CATEGORY_STATUS)
        .build()
      NotificationManagerCompat.from(reactApplicationContext).notify(id, notification)
      promise.resolve(null)
    } catch (error: Exception) {
      promise.reject("DOWNLOAD_NOTIFY_FAILED", error.message ?: "Could not update download status", error)
    }
  }

  @ReactMethod
  fun failDownload(notificationId: Double, message: String, promise: Promise) {
    try {
      ensureChannels()
      val id = notificationId.toInt()
      val text = message.ifBlank { "Download failed" }
      val notification = NotificationCompat.Builder(reactApplicationContext, CHANNEL_COMPLETE)
        .setContentTitle("Download failed")
        .setContentText(text)
        .setSmallIcon(android.R.drawable.stat_notify_error)
        .setOngoing(false)
        .setAutoCancel(true)
        .setCategory(NotificationCompat.CATEGORY_ERROR)
        .build()
      NotificationManagerCompat.from(reactApplicationContext).notify(id, notification)
      promise.resolve(null)
    } catch (error: Exception) {
      try {
        NotificationManagerCompat.from(reactApplicationContext).cancel(notificationId.toInt())
      } catch (_: Exception) {
      }
      promise.reject("DOWNLOAD_NOTIFY_FAILED", error.message ?: "Could not update download status", error)
    }
  }

  @ReactMethod
  fun saveBase64(fileName: String, mimeType: String, base64: String, promise: Promise) {
    try {
      val bytes = Base64.decode(base64, Base64.DEFAULT)
      val safeName = sanitizeFileName(fileName)
      val result = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        saveWithMediaStore(safeName, mimeType, bytes)
      } else {
        saveLegacy(safeName, bytes)
      }
      promise.resolve(result)
    } catch (error: Exception) {
      promise.reject("DOWNLOAD_SAVE_FAILED", error.message ?: "Could not save file", error)
    }
  }

  /**
   * Stream HTTP GET to [destPath] (avoids Expo downloadAsync hangs on large files).
   * When [notificationId] >= 0, updates the ongoing download notification with real progress.
   * Resolves with { path, status, iv, mime, originalSize }.
   */
  @ReactMethod
  fun downloadToFile(
    url: String,
    destPath: String,
    headers: ReadableMap?,
    notificationId: Double,
    fileName: String,
    promise: Promise
  ) {
    ioExecutor.execute {
      var connection: HttpURLConnection? = null
      val outFile = File(stripFileUri(destPath))
      val notifyId = notificationId.toInt()
      val label = sanitizeFileName(fileName)
      try {
        outFile.parentFile?.mkdirs()
        if (outFile.exists() && !outFile.delete()) {
          promise.reject("DOWNLOAD_FAILED", "Could not replace destination file", null)
          return@execute
        }

        connection = (URL(url).openConnection() as HttpURLConnection).apply {
          requestMethod = "GET"
          connectTimeout = 30_000
          // No short read timeout — large encrypted videos can take many minutes.
          readTimeout = 0
          instanceFollowRedirects = true
          doInput = true
          if (headers != null) {
            val iterator = headers.keySetIterator()
            while (iterator.hasNextKey()) {
              val key = iterator.nextKey()
              val value = headers.getString(key) ?: continue
              setRequestProperty(key, value)
            }
          }
        }

        val status = connection.responseCode
        val headerIv = connection.getHeaderField("X-File-IV") ?: ""
        val headerMime = connection.getHeaderField("X-File-Mime")
          ?: "application/octet-stream"
        val headerOriginal = connection.getHeaderField("X-Original-Size")?.toLongOrNull() ?: 0L
        val contentLength = connection.contentLengthLong.let { if (it < 0) 0L else it }

        if (status < 200 || status >= 300) {
          // Drain error body lightly then reject (keep status for JS 401 retry).
          try {
            connection.errorStream?.use { it.readBytes() }
          } catch (_: Exception) {
          }
          outFile.delete()
          val map = Arguments.createMap().apply {
            putInt("status", status)
            putString("path", "")
            putString("iv", headerIv)
            putString("mime", headerMime)
            putDouble("originalSize", headerOriginal.toDouble())
          }
          // Resolve with status so JS can refresh+retry on 401 without treating as crash.
          if (status == 401) {
            promise.resolve(map)
          } else {
            promise.reject("DOWNLOAD_FAILED", "Download failed ($status)", null)
          }
          return@execute
        }

        val input = connection.inputStream
        var written = 0L
        var lastNotifyAt = 0L
        FileOutputStream(outFile).use { output ->
          val buffer = ByteArray(256 * 1024)
          while (true) {
            val n = input.read(buffer)
            if (n < 0) break
            output.write(buffer, 0, n)
            written += n
            val now = System.currentTimeMillis()
            if (notifyId >= 0 && (now - lastNotifyAt >= 400 || written == contentLength)) {
              lastNotifyAt = now
              publishDownloadProgress(notifyId, label, written, contentLength)
            }
          }
          output.flush()
        }
        input.close()

        if (notifyId >= 0) {
          publishDownloadProgress(notifyId, label, written, max(contentLength, written))
        }

        val map = Arguments.createMap().apply {
          putInt("status", status)
          putString("path", outFile.absolutePath)
          putString("iv", headerIv)
          putString("mime", headerMime)
          putDouble("originalSize", headerOriginal.toDouble())
        }
        promise.resolve(map)
      } catch (error: Exception) {
        try {
          outFile.delete()
        } catch (_: Exception) {
        }
        promise.reject(
          "DOWNLOAD_FAILED",
          error.message ?: "Could not download file",
          error
        )
      } finally {
        try {
          connection?.disconnect()
        } catch (_: Exception) {
        }
      }
    }
  }

  @ReactMethod
  fun updateDownloadProgress(
    notificationId: Double,
    fileName: String,
    bytesWritten: Double,
    bytesTotal: Double,
    promise: Promise
  ) {
    try {
      publishDownloadProgress(
        notificationId.toInt(),
        sanitizeFileName(fileName),
        bytesWritten.toLong(),
        bytesTotal.toLong()
      )
      promise.resolve(null)
    } catch (error: Exception) {
      promise.reject("DOWNLOAD_NOTIFY_FAILED", error.message ?: "Could not update progress", error)
    }
  }

  private fun publishDownloadProgress(
    notificationId: Int,
    fileName: String,
    written: Long,
    total: Long
  ) {
    if (notificationId < 0) return
    ensureChannels()
    val indeterminate = total <= 0L
    val max = if (indeterminate) 0 else total.coerceAtMost(Int.MAX_VALUE.toLong()).toInt()
    val progress = if (indeterminate) {
      0
    } else {
      written.coerceAtMost(total).coerceAtMost(Int.MAX_VALUE.toLong()).toInt()
    }
    val text = if (indeterminate) {
      "${formatBytes(written)} · $fileName"
    } else {
      "${formatBytes(written)} / ${formatBytes(total)} · $fileName"
    }
    val notification = NotificationCompat.Builder(reactApplicationContext, CHANNEL_PROGRESS)
      .setContentTitle("Downloading")
      .setContentText(text)
      .setSmallIcon(android.R.drawable.stat_sys_download)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setProgress(max, progress, indeterminate)
      .setCategory(NotificationCompat.CATEGORY_PROGRESS)
      .build()
    NotificationManagerCompat.from(reactApplicationContext).notify(notificationId, notification)
  }

  private fun formatBytes(bytes: Long): String {
    if (bytes < 1024) return "$bytes B"
    val kb = bytes / 1024.0
    if (kb < 1024) return String.format("%.0f KB", kb)
    val mb = kb / 1024.0
    if (mb < 1024) return String.format("%.1f MB", mb)
    val gb = mb / 1024.0
    return String.format("%.2f GB", gb)
  }

  /** Copy an already-decrypted local file into shared Downloads (no base64 / no full RAM load). */
  @ReactMethod
  fun saveFromPath(fileName: String, mimeType: String, sourcePath: String, promise: Promise) {
    ioExecutor.execute {
      try {
        val source = File(stripFileUri(sourcePath))
        if (!source.isFile) {
          promise.reject("DOWNLOAD_SAVE_FAILED", "Source file is missing", null)
          return@execute
        }
        val safeName = sanitizeFileName(fileName)
        val result = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
          saveWithMediaStoreStream(safeName, mimeType, source)
        } else {
          saveLegacyStream(safeName, source)
        }
        promise.resolve(result)
      } catch (error: Exception) {
        promise.reject("DOWNLOAD_SAVE_FAILED", error.message ?: "Could not save file", error)
      }
    }
  }

  /**
   * Stream AES-256-GCM decrypt from [encryptedPath] to [outputPath].
   * Matches @noble/ciphers GCM: 12-byte IV, 16-byte tag appended to ciphertext.
   * Peak RAM is the I/O buffer (~256 KiB), not the file size.
   */
  @ReactMethod
  fun decryptAesGcmFile(
    encryptedPath: String,
    outputPath: String,
    keyB64: String,
    ivB64: String,
    promise: Promise
  ) {
    ioExecutor.execute {
      var outFile: File? = null
      try {
        val keyBytes = decodeFlexibleBase64(keyB64)
        val ivBytes = decodeFlexibleBase64(ivB64)
        if (keyBytes.size != 16 && keyBytes.size != 24 && keyBytes.size != 32) {
          promise.reject("DECRYPT_FAILED", "Invalid AES key length (${keyBytes.size})", null)
          return@execute
        }
        if (ivBytes.isEmpty()) {
          promise.reject("DECRYPT_FAILED", "Missing IV", null)
          return@execute
        }

        val encFile = File(stripFileUri(encryptedPath))
        if (!encFile.isFile) {
          promise.reject("DECRYPT_FAILED", "Encrypted file is missing", null)
          return@execute
        }
        if (encFile.length() < 16L) {
          promise.reject("DECRYPT_FAILED", "Encrypted file is too short", null)
          return@execute
        }

        outFile = File(stripFileUri(outputPath))
        outFile.parentFile?.mkdirs()
        if (outFile.exists() && !outFile.delete()) {
          promise.reject("DECRYPT_FAILED", "Could not replace output file", null)
          return@execute
        }

        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(
          Cipher.DECRYPT_MODE,
          SecretKeySpec(keyBytes, "AES"),
          GCMParameterSpec(128, ivBytes)
        )

        FileInputStream(encFile).use { input ->
          FileOutputStream(outFile).use { output ->
            val buffer = ByteArray(256 * 1024)
            while (true) {
              val n = input.read(buffer)
              if (n < 0) break
              val chunk = cipher.update(buffer, 0, n)
              if (chunk != null && chunk.isNotEmpty()) {
                output.write(chunk)
              }
            }
            val tail = cipher.doFinal()
            if (tail != null && tail.isNotEmpty()) {
              output.write(tail)
            }
            output.flush()
          }
        }

        promise.resolve(outFile.absolutePath)
      } catch (error: Exception) {
        try {
          outFile?.delete()
        } catch (_: Exception) {
        }
        promise.reject(
          "DECRYPT_FAILED",
          error.message ?: "Could not decrypt file",
          error
        )
      }
    }
  }

  private fun ensureChannels() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val manager = reactApplicationContext.getSystemService(NotificationManager::class.java) ?: return

    val progress = NotificationChannel(
      CHANNEL_PROGRESS,
      "Downloads in progress",
      NotificationManager.IMPORTANCE_LOW
    ).apply {
      description = "Shows progress while FreeDrive downloads a file"
      setShowBadge(false)
    }
    val complete = NotificationChannel(
      CHANNEL_COMPLETE,
      "Downloads",
      NotificationManager.IMPORTANCE_DEFAULT
    ).apply {
      description = "Notifies when a FreeDrive download finishes"
    }
    manager.createNotificationChannel(progress)
    manager.createNotificationChannel(complete)
  }

  private fun saveWithMediaStore(
    fileName: String,
    mimeType: String,
    bytes: ByteArray
  ): String {
    val resolver = reactApplicationContext.contentResolver
    val values = pendingDownloadValues(fileName, mimeType)
    val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
      ?: throw IllegalStateException("Android could not create the download")

    try {
      resolver.openOutputStream(uri, "w")?.use { output ->
        output.write(bytes)
        output.flush()
      } ?: throw IllegalStateException("Android could not open the download")
      markDownloadComplete(uri)
      return uri.toString()
    } catch (error: Exception) {
      resolver.delete(uri, null, null)
      throw error
    }
  }

  private fun saveWithMediaStoreStream(
    fileName: String,
    mimeType: String,
    source: File
  ): String {
    val resolver = reactApplicationContext.contentResolver
    val values = pendingDownloadValues(fileName, mimeType)
    val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
      ?: throw IllegalStateException("Android could not create the download")

    try {
      resolver.openOutputStream(uri, "w")?.use { output ->
        FileInputStream(source).use { input ->
          input.copyTo(output, 256 * 1024)
        }
        output.flush()
      } ?: throw IllegalStateException("Android could not open the download")
      markDownloadComplete(uri)
      return uri.toString()
    } catch (error: Exception) {
      resolver.delete(uri, null, null)
      throw error
    }
  }

  private fun pendingDownloadValues(fileName: String, mimeType: String): ContentValues {
    return ContentValues().apply {
      put(MediaStore.Downloads.DISPLAY_NAME, fileName)
      put(
        MediaStore.Downloads.MIME_TYPE,
        mimeType.ifBlank { "application/octet-stream" }
      )
      put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
      put(MediaStore.Downloads.IS_PENDING, 1)
    }
  }

  private fun markDownloadComplete(uri: Uri) {
    val completed = ContentValues().apply {
      put(MediaStore.Downloads.IS_PENDING, 0)
    }
    reactApplicationContext.contentResolver.update(uri, completed, null, null)
  }

  @Suppress("DEPRECATION")
  private fun saveLegacy(fileName: String, bytes: ByteArray): String {
    val target = prepareLegacyTarget(fileName)
    FileOutputStream(target).use { output ->
      output.write(bytes)
      output.flush()
    }
    return target.toURI().toString()
  }

  @Suppress("DEPRECATION")
  private fun saveLegacyStream(fileName: String, source: File): String {
    val target = prepareLegacyTarget(fileName)
    FileInputStream(source).use { input ->
      FileOutputStream(target).use { output ->
        input.copyTo(output, 256 * 1024)
      }
    }
    return target.toURI().toString()
  }

  @Suppress("DEPRECATION")
  private fun prepareLegacyTarget(fileName: String): File {
    val downloads = Environment.getExternalStoragePublicDirectory(
      Environment.DIRECTORY_DOWNLOADS
    )
    if (!downloads.exists() && !downloads.mkdirs()) {
      throw IllegalStateException("Could not create the Downloads folder")
    }
    return uniqueFile(downloads, fileName)
  }

  private fun stripFileUri(path: String): String {
    if (!path.startsWith("file:")) return path
    val parsed = Uri.parse(path).path
    return parsed ?: path.removePrefix("file://")
  }

  private fun decodeFlexibleBase64(value: String): ByteArray {
    val trimmed = value.trim()
    val standard = trimmed
      .replace('-', '+')
      .replace('_', '/')
    val pad = (4 - standard.length % 4) % 4
    val padded = standard + "=".repeat(pad)
    return Base64.decode(padded, Base64.DEFAULT)
  }

  private fun uniqueFile(directory: File, fileName: String): File {
    val initial = File(directory, fileName)
    if (!initial.exists()) return initial

    val dot = fileName.lastIndexOf('.')
    val stem = if (dot > 0) fileName.substring(0, dot) else fileName
    val extension = if (dot > 0) fileName.substring(dot) else ""
    var index = 1
    while (true) {
      val candidate = File(directory, "$stem ($index)$extension")
      if (!candidate.exists()) return candidate
      index += 1
    }
  }

  private fun sanitizeFileName(fileName: String): String {
    val safe = fileName.replace(Regex("""[\\/:*?"<>|]"""), "_").trim()
    return safe.ifBlank { "file" }
  }

  companion object {
    private const val CHANNEL_PROGRESS = "freedrive_downloads_progress"
    private const val CHANNEL_COMPLETE = "freedrive_downloads"
    private val nextNotificationId = AtomicInteger(1000)
  }
}
