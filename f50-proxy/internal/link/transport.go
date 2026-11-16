package link

import (
	"encoding/json"
	"fmt"
	"net/url"
	"slices"
	"strconv"
	"strings"

	"f50/proxy/internal/xrayconf"
)

// streamSettings from share-link queries, and One Node's link extensions
// (mux, fragment, tfo).
type transportResult struct {
	kind, label string // kind for grouping, label for the summary ("REALITY", "WS / TLS")
	stream      obj
}

// transport builds streamSettings (VLESS, Trojan, VMess, SOCKS, HTTP) from the share-link query: type
// (tcp, ws, httpupgrade, grpc, xhttp), security (none, tls, reality) and
// their fields, as vless-link-contract.md lists them.
func transport(q url.Values, host, defaultSecurity string) (transportResult, error) {
	network := strings.ToLower(or(q.Get("type"), "tcp"))
	switch network {
	case "raw":
		network = "tcp"
	case "splithttp":
		network = "xhttp"
	}
	security := strings.ToLower(or(q.Get("security"), defaultSecurity))
	if security == "" {
		security = "none"
	}
	stream := obj{"network": network, "security": security}
	r := transportResult{kind: network}
	var parts []string

	switch network {
	case "tcp":
		if h := q.Get("headerType"); h != "" && h != "none" {
			return r, fmt.Errorf("TCP %s 伪装不支持", h)
		}
	case "ws", "httpupgrade":
		x := obj{"path": or(q.Get("path"), "/")}
		if h := q.Get("host"); h != "" {
			x["host"] = h
		}
		stream[network+"Settings"] = x
		parts = append(parts, network)
	case "grpc":
		g := obj{"serviceName": q.Get("serviceName")}
		switch mode := q.Get("mode"); mode {
		case "", "gun":
		case "multi":
			g["multiMode"] = true
		default:
			return r, fmt.Errorf("grpc mode %q 无效（gun 或 multi）", mode)
		}
		if a := q.Get("authority"); a != "" {
			g["authority"] = a
		}
		stream["grpcSettings"] = g
		parts = append(parts, "GRPC")
	case "xhttp":
		x := obj{"path": or(q.Get("path"), "/"), "mode": or(q.Get("mode"), "auto")}
		if h := q.Get("host"); h != "" {
			x["host"] = h
		}
		if extra := q.Get("extra"); extra != "" {
			if !json.Valid([]byte(extra)) {
				return r, fmt.Errorf("xhttp extra is not JSON")
			}
			x["extra"] = json.RawMessage(extra)
		}
		stream["xhttpSettings"] = x
		parts = append(parts, "XHTTP")
	default:
		return r, fmt.Errorf("transport %q not supported", network)
	}

	switch security {
	case "none":
	case "tls":
		if q.Get("allowInsecure") == "1" || q.Get("insecure") == "1" {
			return r, fmt.Errorf("Xray v26 不再跳过证书校验（allowInsecure），请改用 pcs 固定证书指纹")
		}
		fp, err := fingerprint(q.Get("fp"))
		if err != nil {
			return r, err
		}
		sni := or(q.Get("sni"), or(q.Get("peer"), or(q.Get("host"), host)))
		tls := obj{"serverName": sni, "fingerprint": fp}
		if alpn := q.Get("alpn"); alpn != "" {
			tls["alpn"] = strings.Split(alpn, ",")
		}
		for param, field := range map[string]string{"pcs": "pinnedPeerCertSha256", "vcn": "verifyPeerCertByName", "ech": "echConfigList"} {
			if v := q.Get(param); v != "" {
				tls[field] = v
			}
		}
		stream["tlsSettings"] = tls
		parts = append(parts, "TLS")
		if r.kind == "tcp" {
			r.kind = "tls"
		}
	case "reality":
		if network == "ws" || network == "httpupgrade" {
			return r, fmt.Errorf("REALITY 只支持 tcp、xhttp、grpc 传输，不支持 %s", network)
		}
		fp, err := fingerprint(q.Get("fp"))
		if err != nil {
			return r, err
		}
		rs := obj{
			"serverName":  q.Get("sni"),
			"fingerprint": fp,
			"publicKey":   q.Get("pbk"),
			"shortId":     q.Get("sid"),
			"spiderX":     or(q.Get("spx"), "/"),
		}
		if pqv := q.Get("pqv"); pqv != "" {
			rs["mldsa65Verify"] = pqv
		}
		stream["realitySettings"] = rs
		parts = append([]string{"REALITY"}, parts...)
		if r.kind == "tcp" {
			r.kind = "reality"
		}
	default:
		return r, fmt.Errorf("security %q not supported", security)
	}
	r.label = strings.Join(parts, " / ")
	r.stream = stream
	return r, nil
}

// fingerprints are the uTLS presets a link may name (empty: chrome).
var fingerprints = []string{"chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized"}

func fingerprint(fp string) (string, error) {
	fp = strings.ToLower(or(fp, "chrome"))
	if !slices.Contains(fingerprints, fp) {
		return "", fmt.Errorf("指纹 fp=%s 不支持（可用 %s）", fp, strings.Join(fingerprints, "、"))
	}
	return fp, nil
}

// extensions applies One Node's link parameters beyond Xray's standard:
// mux=<concurrency>, tfo=1 and fragment=<length>,<interval>,<packets>.
func (p *Parsed) extensions(q url.Values, flow string) error {
	if v := q.Get("mux"); v != "" && v != "0" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 || n > 1024 {
			return fmt.Errorf("mux=%s 无效（1-1024）", v)
		}
		if flow != "" {
			return fmt.Errorf("mux 不能和 flow %s 同时使用", flow)
		}
		p.Outbound["mux"] = obj{"enabled": true, "concurrency": n}
		p.Summary += " / MUX"
	}
	if v := q.Get("fragment"); v != "" {
		stream := p.Outbound["streamSettings"].(obj)
		if stream["security"] == "none" {
			p.Warnings = append(p.Warnings, "fragment 只用于 TLS/REALITY，已忽略")
		} else {
			fragment, err := parseFragment(v)
			if err != nil {
				return err
			}
			p.Dialer = obj{"protocol": "freedom", "settings": obj{"fragment": fragment}}
			p.Summary += " / FRAGMENT"
		}
	}
	if v := q.Get("tfo"); v == "1" || v == "true" {
		// On the outbound that opens the socket: the dialer when there is one.
		if p.Dialer != nil {
			xrayconf.Sockopt(p.Dialer)["tcpFastOpen"] = true
		} else {
			xrayconf.Sockopt(p.Outbound)["tcpFastOpen"] = true
		}
	}
	return nil
}

// parseFragment reads "40-60,30-50,tlshello": ClientHello piece lengths, the
// pause between pieces (ms) and which packets to split (tlshello or 1-3).
func parseFragment(v string) (obj, error) {
	fields := strings.Split(v, ",")
	if len(fields) != 3 {
		return nil, fmt.Errorf("fragment=%s 需要 <长度>,<间隔>,<packets>", v)
	}
	for i, f := range fields {
		fields[i] = strings.TrimSpace(f)
	}
	length, interval, packets := fields[0], fields[1], fields[2]
	if !isRange(length) || !isRange(interval) || (packets != "tlshello" && !isRange(packets)) {
		return nil, fmt.Errorf("fragment=%s 无效：长度和间隔写 40 或 40-60，packets 写 tlshello 或 1-3", v)
	}
	return obj{"packets": packets, "length": length, "interval": interval}, nil
}

// isRange is "n" or "n-m" with 0 ≤ n ≤ m.
func isRange(s string) bool {
	lo, hi, found := strings.Cut(s, "-")
	a, err1 := strconv.Atoi(lo)
	b, err2 := a, error(nil)
	if found {
		b, err2 = strconv.Atoi(hi)
	}
	return err1 == nil && err2 == nil && a >= 0 && a <= b
}
