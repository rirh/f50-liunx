package api

import (
	"context"
	"encoding/base64"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestSubscriptionHeaders(t *testing.T) {
	// As the subgateway renders them (render/profile.go).
	u := parseUsage("upload=9663676416; download=39964416819; total=0; expire=0")
	if u == nil || u.Upload != 9663676416 || u.Download != 39964416819 || u.Total != 0 || u.Expire != 0 {
		t.Fatalf("usage %+v", u)
	}
	if parseUsage("") != nil {
		t.Fatal("empty header must give no usage")
	}
	if got := parseTitle("base64:" + base64.StdEncoding.EncodeToString([]byte("_satoshi_nakamoto"))); got != "_satoshi_nakamoto" {
		t.Fatalf("title %q", got)
	}
	if got := parseTitle("plain"); got != "plain" {
		t.Fatalf("title %q", got)
	}
}

// Failures name their kind in plain words, keep the underlying text, and never
// repeat the URL (a subscription link carries the user's token).
func TestFetchErrorIsSpecific(t *testing.T) {
	get := func(ctx context.Context, raw string) error {
		req, _ := http.NewRequestWithContext(ctx, http.MethodGet, raw, nil)
		resp, err := (&http.Client{}).Do(req)
		if err == nil {
			resp.Body.Close()
			t.Fatalf("GET %s succeeded", raw)
		}
		return fetchError("拉取订阅", err)
	}

	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		<-r.Context().Done()
	}))
	defer slow.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	msg := get(ctx, slow.URL+"/sub?token=SECRET").Error()
	if !strings.HasPrefix(msg, "拉取订阅超时（20 秒内没有响应）: ") || !strings.Contains(msg, "deadline exceeded") {
		t.Errorf("timeout: %q", msg)
	}
	if strings.Contains(msg, "SECRET") {
		t.Errorf("timeout message leaks the URL: %q", msg)
	}

	// A port nobody listens on.
	l, _ := net.Listen("tcp", "127.0.0.1:0")
	addr := l.Addr().String()
	l.Close()
	msg = get(context.Background(), "http://"+addr+"/sub?token=SECRET").Error()
	if !strings.HasPrefix(msg, "拉取订阅失败，服务器拒绝连接: ") || !strings.Contains(msg, "connection refused") {
		t.Errorf("refused: %q", msg)
	}
	if strings.Contains(msg, "SECRET") {
		t.Errorf("refused message leaks the URL: %q", msg)
	}
}

func TestStatusError(t *testing.T) {
	resp := &http.Response{Status: "403 Forbidden"}
	if got := statusError("订阅", resp, []byte(" token\n expired ")).Error(); got != "订阅返回 HTTP 403 Forbidden: token expired" {
		t.Errorf("got %q", got)
	}
	// An HTML error page is noise, not a message.
	if got := statusError("订阅", resp, []byte("<html><body>nope</body></html>")).Error(); got != "订阅返回 HTTP 403 Forbidden" {
		t.Errorf("got %q", got)
	}
}
