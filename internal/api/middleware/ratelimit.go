package middleware

import (
	"net/http"
	"sync"
	"time"
)

// RateLimiter provides simple token-bucket rate limiting per IP.
type RateLimiter struct {
	mu       sync.Mutex
	visitors map[string]*visitor
	rate     int
	burst    int
}

type visitor struct {
	tokens    int
	lastCheck time.Time
}

// NewRateLimiter creates a rate limiter.
func NewRateLimiter(rate, burst int) *RateLimiter {
	rl := &RateLimiter{
		visitors: make(map[string]*visitor),
		rate:     rate,
		burst:    burst,
	}
	// Cleanup old visitors periodically
	go func() {
		for {
			time.Sleep(5 * time.Minute)
			rl.mu.Lock()
			for ip, v := range rl.visitors {
				if time.Since(v.lastCheck) > 10*time.Minute {
					delete(rl.visitors, ip)
				}
			}
			rl.mu.Unlock()
		}
	}()
	return rl
}

// Limit returns rate limiting middleware.
func (rl *RateLimiter) Limit(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ip := r.RemoteAddr

		rl.mu.Lock()
		v, exists := rl.visitors[ip]
		if !exists {
			v = &visitor{tokens: rl.burst, lastCheck: time.Now()}
			rl.visitors[ip] = v
		}

		// Refill tokens
		elapsed := time.Since(v.lastCheck)
		v.tokens += int(elapsed.Seconds()) * rl.rate
		if v.tokens > rl.burst {
			v.tokens = rl.burst
		}
		v.lastCheck = time.Now()

		if v.tokens <= 0 {
			rl.mu.Unlock()
			http.Error(w, `{"error":"rate limit exceeded"}`, http.StatusTooManyRequests)
			return
		}

		v.tokens--
		rl.mu.Unlock()

		next.ServeHTTP(w, r)
	})
}
