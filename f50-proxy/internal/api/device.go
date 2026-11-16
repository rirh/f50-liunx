package api

import (
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"os"
	"strings"
	"sync"
	"unicode"
)

// device identifies the router to a subscription gateway. The One Node
// gateway counts devices against a subscription's device limit by X-HWID;
// others ignore the headers.
type device struct {
	ID    string // hashed /etc/machine-id; empty when there is none (stock OpenWrt has none)
	OS    string
	Model string
}

var thisDevice = sync.OnceValue(func() device {
	d := device{OS: "OpenWrt", Model: "ZTE F50"}
	if raw, err := os.ReadFile("/etc/machine-id"); err == nil {
		if id := strings.TrimSpace(string(raw)); id != "" {
			d.ID = deviceID(id)
		}
	}
	return d
})

// deviceID hashes the raw machine ID so the gateway never sees it, and so the
// value means nothing elsewhere.
func deviceID(machine string) string {
	sum := sha256.Sum256([]byte("f50-proxy:" + strings.ToUpper(strings.TrimSpace(machine))))
	return hex.EncodeToString(sum[:])
}

// setHeaders follows the X-HWID convention (Remnawave, Happ) that the gateway reads.
func (d device) setHeaders(h http.Header) {
	for key, value := range map[string]string{"X-HWID": d.ID, "X-Device-OS": d.OS, "X-Device-Model": d.Model} {
		if value = headerValue(value); value != "" {
			h.Set(key, value)
		}
	}
}

// headerValue drops control characters and keeps the gateway's 120-rune limit.
func headerValue(s string) string {
	s = strings.TrimSpace(strings.Map(func(r rune) rune {
		if unicode.IsControl(r) {
			return -1
		}
		return r
	}, s))
	if runes := []rune(s); len(runes) > 120 {
		s = string(runes[:120])
	}
	return s
}
