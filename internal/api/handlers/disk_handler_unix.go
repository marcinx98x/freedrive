//go:build !windows

package handlers

import (
	"net/http"
	"syscall"
)

// DiskStats handles GET /api/v1/disk-stats — returns actual disk space.
func DiskStats(w http.ResponseWriter, r *http.Request) {
	var stat syscall.Statfs_t

	if err := syscall.Statfs(".", &stat); err != nil {
		writeError(w, "failed to get disk stats", http.StatusInternalServerError)
		return
	}

	totalBytes := stat.Blocks * uint64(stat.Bsize)
	freeBytes := stat.Bfree * uint64(stat.Bsize)
	usedBytes := totalBytes - freeBytes

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"total_bytes": totalBytes,
		"used_bytes":  usedBytes,
		"free_bytes":  freeBytes,
	})
}
