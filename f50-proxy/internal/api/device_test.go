package api

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"f50/proxy/internal/engine"
	"f50/proxy/internal/version"
)

func TestDeviceIDIsStableHashedAndScoped(t *testing.T) {
	a := deviceID("564D2D3C-0000-1111-2222-333344445555")
	if len(a) != 64 || strings.Contains(a, "564D2D3C") {
		t.Fatalf("device ID must be a SHA-256 hex digest, got %q", a)
	}
	if b := deviceID(" 564d2d3c-0000-1111-2222-333344445555\n"); a != b {
		t.Fatalf("case and spacing must not change the ID: %s vs %s", a, b)
	}
	if deviceID("other-machine") == a {
		t.Fatal("different machines must differ")
	}
}

func TestHeaderValueDropsControlCharactersAndTruncates(t *testing.T) {
	if got := headerValue(" 张三的 MacBook\r\nX-Evil: 1 "); got != "张三的 MacBookX-Evil: 1" {
		t.Fatalf("got %q", got)
	}
	if got := headerValue(strings.Repeat("机", 200)); len([]rune(got)) != 120 {
		t.Fatalf("want 120 runes, got %d", len([]rune(got)))
	}
}

func TestThisDeviceNamesTheRouter(t *testing.T) {
	d := thisDevice()
	if d.OS != "OpenWrt" || d.Model != "ZTE F50" {
		t.Fatalf("device %+v", d)
	}
	if d.ID != "" && len(d.ID) != 64 {
		t.Fatalf("device ID must be empty or a SHA-256 hex digest, got %q", d.ID)
	}
}

func TestFetchSendsDeviceAndExplainsGatewayRefusal(t *testing.T) {
	var got http.Header
	refuse := false
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		got = r.Header.Clone()
		if refuse {
			// What the gateway sends: a placeholder profile plus the code.
			w.Header().Set("X-One-Node-Error", "device_limit")
			w.Header().Set("Profile-Title", "设备数量已达上限")
			_, _ = w.Write([]byte("c29ja3M6Ly9pbnZhbGlk"))
			return
		}
		_, _ = w.Write([]byte("vless://id@example.com:443#a"))
	}))
	defer srv.Close()
	s := &Server{engine: engine.New("warning")}

	sub, err := s.fetch(context.Background(), srv.URL)
	if err != nil || sub.Body == "" {
		t.Fatalf("fetch: %v", err)
	}
	if got.Get("User-Agent") != "v2rayN/7.15 F50Proxy/"+version.Version {
		t.Fatalf("user agent %q", got.Get("User-Agent"))
	}
	if d := thisDevice(); d.ID != "" && got.Get("X-HWID") != d.ID {
		t.Fatalf("X-HWID %q, want %q", got.Get("X-HWID"), d.ID)
	}
	if got.Get("X-Device-OS") != "OpenWrt" || got.Get("X-Device-Model") != "ZTE F50" {
		t.Fatalf("device headers %q %q", got.Get("X-Device-OS"), got.Get("X-Device-Model"))
	}

	refuse = true
	if _, err := s.fetch(context.Background(), srv.URL); err == nil || !strings.Contains(err.Error(), "设备数量已满") {
		t.Fatalf("want the device limit explained, got %v", err)
	}
}
