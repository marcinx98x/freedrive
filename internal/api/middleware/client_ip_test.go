package middleware

import (
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
)

func TestClientIPIgnoresUntrustedForwarding(t *testing.T) {
	t.Setenv("FREEDRIVE_TRUSTED_PROXIES", "")
	r := httptest.NewRequest(http.MethodGet, "/", nil)
	r.RemoteAddr = "203.0.113.10:1234"
	r.Header.Set("X-Forwarded-For", "198.51.100.1")
	if got := ClientIP(r); got != "203.0.113.10" {
		t.Fatalf("got %q, want peer IP", got)
	}
}

func TestClientIPUsesForwardedWhenTrusted(t *testing.T) {
	t.Setenv("FREEDRIVE_TRUSTED_PROXIES", "10.0.0.0/8")
	r := httptest.NewRequest(http.MethodGet, "/", nil)
	r.RemoteAddr = "10.0.0.5:443"
	r.Header.Set("X-Forwarded-For", "198.51.100.1, 10.0.0.5")
	if got := ClientIP(r); got != "198.51.100.1" {
		t.Fatalf("got %q, want client from XFF", got)
	}
	_ = os.Unsetenv("FREEDRIVE_TRUSTED_PROXIES")
}
