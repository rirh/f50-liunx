// Package probe measures a node's delay: an HTTP request through it, a TCP
// handshake with its port, or an ICMP echo to its server.
package probe

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"strconv"
	"time"

	"f50/proxy/internal/link"
	"f50/proxy/internal/logtap"
	"f50/proxy/internal/netmark"
	"f50/proxy/internal/xrayconf"

	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/core"
)

// Delay times one request to the default test URL through the node (check CLI, tests).
func Delay(ctx context.Context, shareLink string) (int64, error) {
	return URL(ctx, shareLink, nil, "")
}

// Host and Port make the default test URL (tests point them at a loopback server).
var (
	Host = "www.gstatic.com"
	Port = 80
)

// Options is how 设置 → 测速 measures a node.
type Options struct {
	// url (default): one HTTP request through the node. tcp: a handshake with the
	// node's port. icmp: one echo to the node's server. The last two skip the proxy.
	Method string
	// The url method's target; empty means http://www.gstatic.com/generate_204.
	URL string
	// Via are the node's front nodes (前置代理), nearest first.
	Via []string
}

// Node measures one node the chosen way, in milliseconds. Hysteria2 only
// listens on UDP, so tcp falls back to url for it; a chained node is only
// reached through its front nodes, so it is always measured with url.
func Node(ctx context.Context, shareLink string, opts Options) (int64, error) {
	p, err := link.Parse(shareLink)
	if err != nil {
		return 0, err
	}
	method := opts.Method
	if (method == "tcp" && p.Kind == "hy2") || len(opts.Via) > 0 {
		method = "url"
	}
	switch method {
	case "tcp":
		return probeTCP(ctx, p.Server, p.Port)
	case "icmp":
		return probeICMP(ctx, p.Server)
	default:
		return URL(ctx, shareLink, opts.Via, opts.URL)
	}
}

// MaxURLProbes caps the URL probes running at once in this process: each
// one is a whole temporary Xray instance, and a group test or a failover
// may ask for dozens together. The rest wait for a turn (within their own
// deadline). f50d keeps to the same number on its side.
const MaxURLProbes = 3

var urlSlots = make(chan struct{}, MaxURLProbes)

// URL opens a fresh connection through the node and times one HTTP(S)
// request, handshakes included, like the delay tests in Clash-style clients.
// Any reply below 400 counts; redirects are not followed.
func URL(ctx context.Context, shareLink string, via []string, target string) (int64, error) {
	select {
	case urlSlots <- struct{}{}:
		defer func() { <-urlSlots }()
	case <-ctx.Done():
		return 0, errors.New("timeout")
	}
	_, outbounds, err := link.ChainOutbounds(shareLink, via, "proxy")
	if err != nil {
		return 0, err
	}
	xrayconf.MarkSockets(outbounds, netmark.Get())
	config, err := xrayconf.Build(xrayconf.ProbeDocument(outbounds...))
	if err != nil {
		return 0, err
	}
	inst, err := core.New(config)
	// The probe's own logger just replaced the process-wide one.
	logtap.Reinstall()
	if err != nil {
		return 0, err
	}
	if err := inst.Start(); err != nil {
		_ = inst.Close()
		return 0, err
	}
	defer inst.Close()

	return HTTPDelay(ctx, func(ctx context.Context, dest xnet.Destination) (net.Conn, error) {
		return core.Dial(ctx, inst, dest)
	}, target)
}

// HTTPDelay times one request over connections from dial.
func HTTPDelay(ctx context.Context, dial func(context.Context, xnet.Destination) (net.Conn, error), target string) (int64, error) {
	if target == "" {
		target = "http://" + net.JoinHostPort(Host, strconv.Itoa(Port)) + "/generate_204"
	}
	client := &http.Client{
		Transport: &http.Transport{
			DisableKeepAlives: true,
			DialContext: func(ctx context.Context, _, addr string) (net.Conn, error) {
				dest, err := xnet.ParseDestination("tcp:" + addr)
				if err != nil {
					return nil, err
				}
				return dial(ctx, dest)
			},
		},
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, target, nil)
	if err != nil {
		return 0, err
	}
	start := time.Now()
	resp, err := client.Do(req)
	if err != nil {
		if ctx.Err() != nil {
			return 0, errors.New("timeout")
		}
		return 0, err
	}
	ms := time.Since(start).Milliseconds()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 64<<10))
	_ = resp.Body.Close()
	if resp.StatusCode >= 400 {
		return 0, fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	return max(ms, 1), nil
}

// probeTCP times a TCP handshake with the node's port, straight from the router.
func probeTCP(ctx context.Context, host string, port int) (int64, error) {
	ip, err := resolveHost(ctx, host)
	if err != nil {
		return 0, err
	}
	d := netmark.Dialer(0)
	start := time.Now()
	conn, err := d.DialContext(ctx, "tcp", net.JoinHostPort(ip.String(), strconv.Itoa(port)))
	if err != nil {
		if ctx.Err() != nil {
			return 0, errors.New("timeout")
		}
		return 0, err
	}
	ms := time.Since(start).Milliseconds()
	_ = conn.Close()
	return max(ms, 1), nil
}

// resolveHost looks the server up before timing starts, preferring IPv4
// (what most nodes serve).
func resolveHost(ctx context.Context, host string) (net.IP, error) {
	if ip := net.ParseIP(host); ip != nil {
		return ip, nil
	}
	ips, err := netmark.Resolver().LookupIP(ctx, "ip", host)
	if err != nil {
		return nil, err
	}
	for _, ip := range ips {
		if ip.To4() != nil {
			return ip, nil
		}
	}
	if len(ips) == 0 {
		return nil, fmt.Errorf("%s: no address", host)
	}
	return ips[0], nil
}
