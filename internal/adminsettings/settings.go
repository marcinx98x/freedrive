package adminsettings

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

var settingsFile = "data/settings.json"

// SetDataDir points settings reads at dataDir/settings.json (call from main).
func SetDataDir(dataDir string) {
	if strings.TrimSpace(dataDir) != "" {
		settingsFile = filepath.Join(dataDir, "settings.json")
	}
}

func load() map[string]interface{} {
	out := map[string]interface{}{}
	if err := os.MkdirAll(filepath.Dir(settingsFile), 0755); err != nil {
		return out
	}
	bytes, err := os.ReadFile(settingsFile)
	if err != nil {
		return out
	}
	_ = json.Unmarshal(bytes, &out)
	return out
}

func general(data map[string]interface{}) map[string]interface{} {
	g, _ := data["general"].(map[string]interface{})
	if g == nil {
		return map[string]interface{}{}
	}
	return g
}

func storage(data map[string]interface{}) map[string]interface{} {
	s, _ := data["storage"].(map[string]interface{})
	if s == nil {
		return map[string]interface{}{}
	}
	return s
}

func asString(v interface{}) string {
	switch t := v.(type) {
	case string:
		return strings.TrimSpace(t)
	case float64:
		return strconv.FormatInt(int64(t), 10)
	case int:
		return strconv.Itoa(t)
	case int64:
		return strconv.FormatInt(t, 10)
	default:
		return ""
	}
}

func asInt(v interface{}, fallback int) int {
	switch t := v.(type) {
	case float64:
		return int(t)
	case int:
		return t
	case int64:
		return int(t)
	case string:
		if n, err := strconv.Atoi(strings.TrimSpace(t)); err == nil {
			return n
		}
	}
	return fallback
}

// RegistrationMode returns open, invite, or closed (default invite).
func RegistrationMode() string {
	mode := strings.ToLower(asString(general(load())["registration"]))
	switch mode {
	case "open", "closed":
		return mode
	default:
		return "invite"
	}
}

// DefaultQuotaBytes returns the configured default user quota.
func DefaultQuotaBytes() int64 {
	gb := asInt(general(load())["default_quota_gb"], 10)
	if gb <= 0 {
		gb = 10
	}
	return int64(gb) * 1024 * 1024 * 1024
}

// MaxUploadMB returns admin max upload size in megabytes (0 if unset).
func MaxUploadMB() int {
	return asInt(general(load())["max_upload_mb"], 0)
}

// AllowedTypes returns lowercase file extensions without dots.
func AllowedTypes() []string {
	raw, ok := general(load())["allowed_types"].([]interface{})
	if !ok || len(raw) == 0 {
		return nil
	}
	out := make([]string, 0, len(raw))
	seen := map[string]bool{}
	for _, item := range raw {
		ext := strings.ToLower(strings.TrimPrefix(strings.TrimSpace(asString(item)), "."))
		if ext == "" || seen[ext] {
			continue
		}
		seen[ext] = true
		out = append(out, ext)
	}
	return out
}

// TrashAutoEmptyDays returns retention days; 0 means never auto-empty.
func TrashAutoEmptyDays() int {
	v := strings.ToLower(asString(storage(load())["trash_auto_empty"]))
	if v == "" || v == "never" {
		return 0
	}
	return asInt(v, 30)
}

// EffectiveMaxUploadBytes picks the larger of config limit and admin setting.
func EffectiveMaxUploadBytes(configLimit int64) int64 {
	mb := MaxUploadMB()
	if mb <= 0 {
		return configLimit
	}
	setting := int64(mb) * 1024 * 1024
	if setting > configLimit {
		return setting
	}
	return configLimit
}
