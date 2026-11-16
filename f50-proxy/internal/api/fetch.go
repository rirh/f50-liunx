package api

import (
	"context"
	"crypto/tls"
	"encoding/base64"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"syscall"
	"time"

	"f50/proxy/internal/netmark"
	"f50/proxy/internal/version"
)

// gatewayErrors explains the One Node gateway's X-One-Node-Error codes. The
// gateway still answers 200 with a placeholder profile for other clients.
var gatewayErrors = map[string]string{
	"device_limit":       "这个订阅的设备数量已满，请联系管理员在后台移除不再使用的设备",
	"device_id_required": "无法读取本机设备标识，这个订阅限制了设备数量",
	"device_id_invalid":  "本机设备标识无效",
	"quota_exceeded":     "订阅流量已用完",
	"token_expired":      "订阅已过期",
	"device_inactive":    "这台设备已被管理员停用，请联系管理员",
	"client_policy":      "订阅的客户端策略拒绝了这台设备，请联系管理员",
}

type subscription struct {
	Body  string
	Title string // Profile-Title, "base64:" prefix decoded
	Usage *Usage // Subscription-Userinfo, when sent
}

// Usage is the de-facto Subscription-Userinfo header: bytes, expire in Unix
// seconds (0 = never).
type Usage struct {
	Upload   int64 `json:"upload"`
	Download int64 `json:"download"`
	Total    int64 `json:"total"`
	Expire   int64 `json:"expire"`
}

func parseUsage(header string) *Usage {
	if header == "" {
		return nil
	}
	u := &Usage{}
	for part := range strings.SplitSeq(header, ";") {
		key, value, ok := strings.Cut(strings.TrimSpace(part), "=")
		if !ok {
			continue
		}
		n, _ := strconv.ParseInt(strings.TrimSpace(value), 10, 64)
		switch strings.TrimSpace(key) {
		case "upload":
			u.Upload = n
		case "download":
			u.Download = n
		case "total":
			u.Total = n
		case "expire":
			u.Expire = n
		}
	}
	return u
}

func parseTitle(header string) string {
	if v, ok := strings.CutPrefix(header, "base64:"); ok {
		if b, err := base64.StdEncoding.DecodeString(v); err == nil {
			return string(b)
		}
	}
	return header
}

// fetch downloads the subscription, through the running node when there is one,
// so a blocked subscription domain still updates once connected.
func (s *Server) fetch(ctx context.Context, raw string) (subscription, error) {
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "https" && u.Scheme != "http") {
		return subscription{}, fmt.Errorf("订阅地址必须是 http(s) 链接")
	}
	client := s.httpClient()
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, raw, nil)
	req.Header.Set("User-Agent", version.SubscriptionUA)
	thisDevice().setHeaders(req.Header)
	resp, err := client.Do(req)
	if err != nil {
		return subscription{}, fetchError("拉取订阅", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 4<<20))
	if err != nil {
		return subscription{}, fetchError("读取订阅内容", err)
	}
	if resp.StatusCode != http.StatusOK {
		return subscription{}, statusError("订阅", resp, body)
	}
	if code := resp.Header.Get("X-One-Node-Error"); code != "" {
		if message, ok := gatewayErrors[code]; ok {
			return subscription{}, errors.New(message)
		}
		if title := parseTitle(resp.Header.Get("Profile-Title")); title != "" {
			return subscription{}, errors.New(title)
		}
		return subscription{}, fmt.Errorf("订阅不可用（%s）", code)
	}
	return subscription{
		Body:  string(body),
		Title: parseTitle(resp.Header.Get("Profile-Title")),
		Usage: parseUsage(resp.Header.Get("Subscription-Userinfo")),
	}, nil
}

// httpClient goes through the running node when there is one.
func (s *Server) httpClient() *http.Client {
	transport := &http.Transport{DialContext: netmark.Dial}
	if st := s.engine.Status(); st.Running {
		transport.Proxy = http.ProxyURL(&url.URL{Scheme: "socks5h", Host: fmt.Sprintf("127.0.0.1:%d", st.Port)})
	}
	return &http.Client{Timeout: fetchTimeout, Transport: transport}
}

const fetchTimeout = 20 * time.Second

// fetchError says in plain words what went wrong with a download (timeout, DNS,
// refused, TLS…) and keeps the underlying error text for the notification. The
// URL is cut from it: a subscription link carries the user's token.
func fetchError(action string, err error) error {
	cause := err
	var ue *url.Error
	if errors.As(err, &ue) {
		cause = ue.Err
	}
	detail := cause.Error()
	var (
		dns  *net.DNSError
		op   *net.OpError
		cert *tls.CertificateVerificationError
		ne   net.Error
	)
	switch {
	case errors.Is(err, context.DeadlineExceeded) || (errors.As(err, &ne) && ne.Timeout()):
		return fmt.Errorf("%s超时（%d 秒内没有响应）: %s", action, int(fetchTimeout.Seconds()), detail)
	case errors.Is(err, context.Canceled):
		return fmt.Errorf("%s被取消: %s", action, detail)
	case errors.As(err, &dns):
		return fmt.Errorf("%s失败，域名解析不了: %s", action, detail)
	case errors.As(err, &cert):
		return fmt.Errorf("%s失败，证书校验不通过: %s", action, detail)
	case errors.Is(err, syscall.ECONNREFUSED):
		return fmt.Errorf("%s失败，服务器拒绝连接: %s", action, detail)
	case errors.Is(err, syscall.ECONNRESET) || errors.Is(err, io.ErrUnexpectedEOF) || errors.Is(err, io.EOF):
		return fmt.Errorf("%s失败，连接被中断: %s", action, detail)
	case errors.As(err, &op) && op.Op == "socks connect":
		return fmt.Errorf("%s失败，当前节点连不上这个地址: %s", action, detail)
	}
	return fmt.Errorf("%s失败: %s", action, detail)
}

// statusError reports a non-200 answer with its status line and the start of the body.
func statusError(what string, resp *http.Response, body []byte) error {
	text := strings.Join(strings.Fields(string(body)), " ")
	if r := []rune(text); len(r) > 120 {
		text = string(r[:120]) + "…"
	}
	if text == "" || strings.HasPrefix(text, "<") {
		return fmt.Errorf("%s返回 HTTP %s", what, resp.Status)
	}
	return fmt.Errorf("%s返回 HTTP %s: %s", what, resp.Status, text)
}

// fetchRules downloads a 配置 from its link (Shadowrocket's config URL). Unlike
// the subscription, no device headers: the host is usually someone else's.
func (s *Server) fetchRules(ctx context.Context, raw string) (string, error) {
	u, err := url.Parse(raw)
	if err != nil || (u.Scheme != "https" && u.Scheme != "http") {
		return "", fmt.Errorf("配置链接必须是 http(s) 链接")
	}
	req, _ := http.NewRequestWithContext(ctx, http.MethodGet, raw, nil)
	req.Header.Set("User-Agent", version.ClientUA)
	resp, err := s.httpClient().Do(req)
	if err != nil {
		return "", fetchError("下载配置", err)
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20))
	if err != nil {
		return "", fetchError("读取配置内容", err)
	}
	if resp.StatusCode != http.StatusOK {
		return "", statusError("配置链接", resp, body)
	}
	if strings.TrimSpace(string(body)) == "" {
		return "", fmt.Errorf("配置链接返回的内容是空的")
	}
	return string(body), nil
}
