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
// gateway counts devices against a subscription's device limit by X-HWID and,
// under its client policy, refuses One Node F50 downloads without one;
// others ignore the headers.
type device struct {
	ID      string // hashed machine ID (machineIDSources); empty when none is readable
	OS      string
	Version string // OpenWrt release (osRelease); empty when unknown
	Model   string
}

// machineIDSources are read in order; the first non-empty one is the device.
// Stock OpenWrt has no /etc/machine-id (and release images must not carry
// one), so the F50's eMMC CID, unique per chip and kept across reflashes,
// stands in.
var machineIDSources = []string{"/etc/machine-id", "/sys/block/mmcblk0/device/cid"}

var thisDevice = sync.OnceValue(func() device {
	return device{
		ID: readMachineID(machineIDSources), OS: "OpenWrt",
		Version: readOSRelease("/etc/openwrt_release"), Model: "ZTE F50",
	}
})

// readOSRelease returns DISTRIB_RELEASE from /etc/openwrt_release ("25.12.6"),
// or "" when the file or the key is missing.
func readOSRelease(path string) string {
	raw, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	for _, line := range strings.Split(string(raw), "\n") {
		if value, ok := strings.CutPrefix(strings.TrimSpace(line), "DISTRIB_RELEASE="); ok {
			return strings.Trim(value, `'"`)
		}
	}
	return ""
}

// readMachineID hashes the first non-empty source ("" when there is none).
func readMachineID(sources []string) string {
	for _, path := range sources {
		if raw, err := os.ReadFile(path); err == nil {
			if id := strings.TrimSpace(string(raw)); id != "" {
				return deviceID(id)
			}
		}
	}
	return ""
}

// deviceID hashes the raw machine ID so the gateway never sees it, and so the
// value means nothing elsewhere.
func deviceID(machine string) string {
	sum := sha256.Sum256([]byte("f50-proxy:" + strings.ToUpper(strings.TrimSpace(machine))))
	return hex.EncodeToString(sum[:])
}

// setHeaders follows the X-HWID convention (Remnawave, Happ) that the gateway reads.
func (d device) setHeaders(h http.Header) {
	for key, value := range map[string]string{
		"X-HWID": d.ID, "X-Device-OS": d.OS, "X-Ver-OS": d.Version, "X-Device-Model": d.Model,
	} {
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
