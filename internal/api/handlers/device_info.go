package handlers

import (
	"net/http"
	"strings"

	"github.com/abdullaabdullazade/freedrive/internal/api/middleware"
	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/service"
)

// deviceInfoFromRequest builds DeviceInfo from headers / User-Agent.
func deviceInfoFromRequest(r *http.Request) service.DeviceInfo {
	ua := r.UserAgent()
	deviceType := strings.ToLower(strings.TrimSpace(r.Header.Get("X-Device-Type")))
	deviceName := strings.TrimSpace(r.Header.Get("X-Device-Name"))

	if deviceType == domain.DeviceTypeDesktop {
		if deviceName == "" {
			deviceName = "Desktop app"
		}
		return service.DeviceInfo{
			DeviceName: deviceName,
			DeviceType: domain.DeviceTypeDesktop,
			UserAgent:  ua,
			IPAddress:  middleware.ClientIP(r),
		}
	}

	if deviceName == "" {
		deviceName = describeUserAgent(ua)
	}
	return service.DeviceInfo{
		DeviceName: deviceName,
		DeviceType: domain.DeviceTypeWeb,
		UserAgent:  ua,
		IPAddress:  middleware.ClientIP(r),
	}
}

func describeUserAgent(ua string) string {
	uaLower := strings.ToLower(ua)
	browser := "Browser"
	switch {
	case strings.Contains(uaLower, "edg/"):
		browser = "Edge"
	case strings.Contains(uaLower, "chrome/") && !strings.Contains(uaLower, "edg/"):
		browser = "Chrome"
	case strings.Contains(uaLower, "firefox/"):
		browser = "Firefox"
	case strings.Contains(uaLower, "safari/") && !strings.Contains(uaLower, "chrome/"):
		browser = "Safari"
	case strings.Contains(uaLower, "opr/") || strings.Contains(uaLower, "opera"):
		browser = "Opera"
	}

	osName := "Unknown OS"
	switch {
	case strings.Contains(uaLower, "windows"):
		osName = "Windows"
	case strings.Contains(uaLower, "mac os") || strings.Contains(uaLower, "macintosh"):
		osName = "macOS"
	case strings.Contains(uaLower, "android"):
		osName = "Android"
	case strings.Contains(uaLower, "iphone") || strings.Contains(uaLower, "ipad"):
		osName = "iOS"
	case strings.Contains(uaLower, "linux"):
		osName = "Linux"
	}

	return browser + " on " + osName
}
