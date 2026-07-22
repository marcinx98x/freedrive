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
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.atomic.AtomicInteger

class DownloadsModule(
  reactContext: ReactApplicationContext
) : ReactContextBaseJavaModule(reactContext) {

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
    val values = ContentValues().apply {
      put(MediaStore.Downloads.DISPLAY_NAME, fileName)
      put(
        MediaStore.Downloads.MIME_TYPE,
        mimeType.ifBlank { "application/octet-stream" }
      )
      put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
      put(MediaStore.Downloads.IS_PENDING, 1)
    }

    val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
      ?: throw IllegalStateException("Android could not create the download")

    try {
      resolver.openOutputStream(uri, "w")?.use { output ->
        output.write(bytes)
        output.flush()
      } ?: throw IllegalStateException("Android could not open the download")

      val completed = ContentValues().apply {
        put(MediaStore.Downloads.IS_PENDING, 0)
      }
      resolver.update(uri, completed, null, null)
      return uri.toString()
    } catch (error: Exception) {
      resolver.delete(uri, null, null)
      throw error
    }
  }

  @Suppress("DEPRECATION")
  private fun saveLegacy(fileName: String, bytes: ByteArray): String {
    val downloads = Environment.getExternalStoragePublicDirectory(
      Environment.DIRECTORY_DOWNLOADS
    )
    if (!downloads.exists() && !downloads.mkdirs()) {
      throw IllegalStateException("Could not create the Downloads folder")
    }

    val target = uniqueFile(downloads, fileName)
    FileOutputStream(target).use { output ->
      output.write(bytes)
      output.flush()
    }
    return target.toURI().toString()
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
