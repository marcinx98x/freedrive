//go:build windows

package handlers

import (
	"net/http"
	"os"
	"path/filepath"

	"golang.org/x/sys/windows"
)

// DiskStats handles GET /api/v1/disk-stats — returns actual disk space.
func DiskStats(w http.ResponseWriter, r *http.Request) {
	wd, err := os.Getwd()
	if err != nil {
		writeError(w, "failed to get working directory", http.StatusInternalServerError)
		return
	}

	volume := filepath.VolumeName(wd)
	if volume == "" {
		volume = `C:`
	}
	rootPath := volume + `\`

	rootPtr, err := windows.UTF16PtrFromString(rootPath)
	if err != nil {
		writeError(w, "failed to resolve disk path", http.StatusInternalServerError)
		return
	}

	var freeAvailable uint64
	var totalBytes uint64
	var freeBytes uint64
	if err := windows.GetDiskFreeSpaceEx(rootPtr, &freeAvailable, &totalBytes, &freeBytes); err != nil {
		writeError(w, "failed to get disk stats", http.StatusInternalServerError)
		return
	}

	usedBytes := totalBytes - freeBytes
	writeJSON(w, http.StatusOK, map[string]interface{}{
		"total_bytes": totalBytes,
		"used_bytes":  usedBytes,
		"free_bytes":  freeBytes,
	})
}
