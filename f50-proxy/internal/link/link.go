// Package link parses share links and subscriptions into nodes and the Xray
// outbounds they run on.
package link

import (
	"encoding/base64"
	"fmt"
	"net/url"
	"slices"
	"strconv"
	"strings"

	"f50/proxy/internal/xrayconf"
)

// obj is an Xray JSON object (xrayconf.Obj).
type obj = xrayconf.Obj

// BindInterface, when set (check --interface), pins the node connection to one
// network interface.
var BindInterface string

// Node is one line from a subscription, as the UI lists it. Link keeps the
// original share link so f50d can persist it and start it later.
type Node struct {
	ID      string `json:"id"`
	Name    string `json:"name"`
	Kind    string `json:"kind"`    // reality | xhttp | tls | tcp | ws | grpc … | hy2 | ss | socks | http
	Summary string `json:"summary"` // "VLESS / REALITY / XTLS-RPRX-VISION / UDP"
	Server  string `json:"server"`
	Port    int    `json:"port"`
	Link    string `json:"link"`
	Problem string `json:"problem,omitempty"`
	// Warnings are parameters the link carries but the node ignores.
	Warnings []string `json:"warnings,omitempty"`
	Protocol string   `json:"protocol"`
}

// ParseSubscription accepts what the One Node subgateway serves for base64
// clients (one base64 blob of share links) or plain share links, one per line.
func ParseSubscription(body string) []Node {
	text := strings.TrimSpace(body)
	if !strings.Contains(text, "://") {
		if decoded, ok := decodeBase64(text); ok {
			text = decoded
		}
	}
	var nodes []Node
	for line := range strings.SplitSeq(text, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		line = normalizeLink(line)
		node := Node{ID: strconv.Itoa(len(nodes)), Link: line}
		p, err := Parse(line)
		node.Protocol, node.Name, node.Server, node.Port, node.Kind, node.Summary = p.Protocol, p.Name, p.Server, p.Port, p.Kind, p.Summary
		node.Warnings = p.Warnings
		if node.Name == "" {
			node.Name = node.Server
		}
		if node.Name == "" {
			node.Name = "无法解析"
		}
		if err == nil {
			// Let Xray itself vet the line now rather than at connect time.
			_, err = xrayconf.Build(xrayconf.ProbeDocument(p.outbounds("proxy")...))
		}
		if err != nil {
			node.Problem = err.Error()
		}
		nodes = append(nodes, node)
	}
	return nodes
}

func decodeBase64(s string) (string, bool) {
	s = strings.Join(strings.Fields(s), "")
	for _, enc := range []*base64.Encoding{base64.StdEncoding, base64.RawStdEncoding, base64.URLEncoding, base64.RawURLEncoding} {
		if b, err := enc.DecodeString(s); err == nil {
			return string(b), true
		}
	}
	return "", false
}

// Parsed is one share link resolved into what the UI shows and what Xray runs.
type Parsed struct {
	Protocol, Name, Server, Kind, Summary string
	Port                                  int
	Outbound                              obj
	// Dialer, when set, is a freedom outbound the node dials through (fragment).
	Dialer   obj
	Warnings []string
}

// Outbound maps a share link to an Xray outbound tagged "proxy" (without the
// dialer a fragment link also needs; see NodeOutbounds).
func Outbound(link string) (string, obj, error) {
	p, err := Parse(link)
	return p.Kind, p.Outbound, err
}

// NodeOutbounds is everything a node runs on, tagged tag: its outbound first,
// then the outbound it dials through, if any (tag + dialerSuffix).
func NodeOutbounds(link, tag string) (string, []obj, error) {
	p, err := Parse(link)
	if err != nil {
		return p.Kind, nil, err
	}
	return p.Kind, p.outbounds(tag), nil
}

const dialerSuffix = ">fragment"

// ViaSuffix + n tags the nth front node (前置代理) of a chained node.
const ViaSuffix = ">via"

// ChainOutbounds is NodeOutbounds for a node reached through front nodes
// (前置代理): via[0] is the node it dials through, via[1] the one via[0] dials
// through, and so on. Each hop opens its connection through the next one
// (Xray's dialerProxy), so only the last hop opens a real socket and only its
// fragment dialer and BindInterface apply. A UDP node (Hysteria2) needs front
// nodes that carry UDP.
func ChainOutbounds(link string, via []string, tag string) (string, []obj, error) {
	p, err := Parse(link)
	if err != nil {
		return p.Kind, nil, err
	}
	hops := []Parsed{p}
	for i, l := range via {
		h, err := Parse(l)
		if err != nil {
			return p.Kind, nil, fmt.Errorf("前置代理 %d: %w", i+1, err)
		}
		hops = append(hops, h)
	}
	var out []obj
	for i, h := range hops {
		hopTag := tag
		if i > 0 {
			hopTag = tag + ViaSuffix + strconv.Itoa(i)
		}
		if i == len(hops)-1 {
			out = append(out, h.outbounds(hopTag)...)
			break
		}
		h.Outbound["tag"] = hopTag
		xrayconf.Sockopt(h.Outbound)["dialerProxy"] = tag + ViaSuffix + strconv.Itoa(i+1)
		out = append(out, h.Outbound)
	}
	return p.Kind, out, nil
}

// outbounds tags the node's outbounds (it changes p's maps: once per parse)
// and binds the one that opens the socket to BindInterface.
func (p Parsed) outbounds(tag string) []obj {
	p.Outbound["tag"] = tag
	dialing := p.Outbound
	out := []obj{p.Outbound}
	if p.Dialer != nil {
		p.Dialer["tag"] = tag + dialerSuffix
		xrayconf.Sockopt(p.Outbound)["dialerProxy"] = tag + dialerSuffix
		dialing = p.Dialer
		out = append(out, p.Dialer)
	}
	if BindInterface != "" {
		xrayconf.Sockopt(dialing)["interface"] = BindInterface
	}
	return out
}

// Parse understands the protocols One Node serves (vless://, hysteria2://
// or hy2://, ss://, socks5://) and the other ones Xray can dial: trojan://,
// vmess://, http:// and https://, in the forms their usual clients export.
func Parse(link string) (Parsed, error) {
	link = normalizeLink(strings.TrimSpace(link))
	i := strings.Index(link, "://")
	if i <= 0 {
		return Parsed{}, fmt.Errorf("not a share link")
	}
	scheme := strings.ToLower(link[:i])
	p := Parsed{Protocol: scheme}
	u, err := url.Parse(link)
	if err == nil {
		// Read name and address first so even unsupported lines list sensibly.
		p.Name, p.Server = u.Fragment, u.Hostname()
		p.Port, _ = strconv.Atoi(u.Port())
	}
	if !slices.Contains(schemes, scheme) {
		return p, fmt.Errorf("scheme %q not supported", scheme)
	}
	if err != nil {
		return p, fmt.Errorf("not a URL")
	}
	if p.Port <= 0 || p.Port > 65535 {
		return p, fmt.Errorf("missing port")
	}
	if p.Server == "" {
		return p, fmt.Errorf("missing address")
	}
	q := u.Query()
	switch scheme {
	case "vless":
		user := obj{"address": p.Server, "port": p.Port, "id": u.User.Username(), "encryption": or(q.Get("encryption"), "none")}
		t, err := transport(q, p.Server, "none")
		if err != nil {
			return p, err
		}
		flow := q.Get("flow")
		if flow != "" && (t.stream["network"] != "tcp" || t.stream["security"] == "none") {
			p.Warnings = append(p.Warnings, fmt.Sprintf("flow %s 只用于 TCP + TLS/REALITY，已忽略", flow))
			flow = ""
		}
		if flow != "" {
			user["flow"] = flow
		}
		p.Kind = t.kind
		p.Summary = summary("VLESS", t.label, flow, encLabel(q.Get("encryption")), "UDP")
		p.Outbound = obj{"tag": "proxy", "protocol": "vless", "settings": user, "streamSettings": t.stream}
		return p, p.extensions(q, flow)
	case "hysteria2", "hy2":
		p.Protocol, p.Kind = "hysteria2", "hy2"
		if q.Get("insecure") == "1" || q.Get("allowInsecure") == "1" {
			return p, fmt.Errorf("hysteria2 insecure=1 needs pinnedPeerCertSha256")
		}
		if obfs := q.Get("obfs"); obfs != "" && obfs != "none" {
			return p, fmt.Errorf("hysteria2 obfs not supported")
		}
		stream := obj{
			"network":          "hysteria",
			"security":         "tls",
			"tlsSettings":      obj{"serverName": or(q.Get("sni"), or(q.Get("peer"), p.Server)), "alpn": []string{"h3"}},
			"hysteriaSettings": obj{"version": 2, "auth": userinfo(u)},
		}
		hop := ""
		if mport := q.Get("mport"); mport != "" {
			hop = "PORT HOPPING"
			stream["finalmask"] = obj{"udp": []obj{{
				"type":     "udphop",
				"settings": obj{"mode": "intervalLocal,intervalRemote", "remotePorts": mport},
			}}}
		}
		p.Summary = summary("HYSTERIA2", hop, "UDP")
		p.Outbound = obj{"tag": "proxy", "protocol": "hysteria", "settings": obj{"version": 2, "address": p.Server, "port": p.Port}, "streamSettings": stream}
		return p, nil
	case "trojan":
		// Trojan is TLS unless the link says otherwise.
		t, err := transport(q, p.Server, "tls")
		if err != nil {
			return p, err
		}
		if flow := q.Get("flow"); flow != "" {
			p.Warnings = append(p.Warnings, fmt.Sprintf("Xray 的 Trojan 不支持 flow %s，已忽略", flow))
		}
		p.Kind = t.kind
		p.Summary = summary("TROJAN", t.label, "UDP")
		p.Outbound = obj{"tag": "proxy", "protocol": "trojan", "settings": obj{"address": p.Server, "port": p.Port, "password": userinfo(u)}, "streamSettings": t.stream}
		return p, p.extensions(q, "")
	case "vmess":
		t, err := transport(q, p.Server, "none")
		if err != nil {
			return p, err
		}
		cipher := strings.ToLower(or(q.Get("encryption"), "auto"))
		if !slices.Contains(vmessCiphers, cipher) {
			return p, fmt.Errorf("VMess 加密 %s 不支持（可用 %s）", cipher, strings.Join(vmessCiphers, "、"))
		}
		if aid := q.Get("aid"); aid != "" && aid != "0" {
			p.Warnings = append(p.Warnings, "Xray 只支持 AEAD VMess，alterId "+aid+" 按 0 连接")
		}
		label := ""
		if cipher != "auto" {
			label = cipher
		}
		p.Kind = t.kind
		p.Summary = summary("VMESS", t.label, label, "UDP")
		p.Outbound = obj{"tag": "proxy", "protocol": "vmess", "settings": obj{"address": p.Server, "port": p.Port, "id": u.User.Username(), "security": cipher}, "streamSettings": t.stream}
		return p, p.extensions(q, "")
	case "ss":
		p.Protocol, p.Kind = "shadowsocks", "ss"
		method := strings.ToLower(u.User.Username())
		password, _ := u.User.Password()
		if method == "" || password == "" {
			return p, fmt.Errorf("Shadowsocks 链接缺少加密方式或密码")
		}
		if plugin := q.Get("plugin"); plugin != "" && plugin != "none" {
			name, _, _ := strings.Cut(plugin, ";")
			return p, fmt.Errorf("Shadowsocks 插件 %s 不支持（Xray 没有 SIP003 插件）", name)
		}
		p.Summary = summary("SHADOWSOCKS", method, "UDP")
		p.Outbound = obj{"tag": "proxy", "protocol": "shadowsocks", "settings": obj{"address": p.Server, "port": p.Port, "method": method, "password": password}}
		return p, p.extensions(q, "")
	case "socks", "socks5", "http", "https":
		protocol, label, fallback := "socks", "SOCKS5", "none"
		p.Protocol, p.Kind = "socks", "socks"
		if scheme == "http" || scheme == "https" {
			// A web address pasted by mistake (a subscription URL) is not a proxy.
			if u.Path != "" && u.Path != "/" {
				return p, fmt.Errorf("这是网址，不是 HTTP 代理（订阅地址请在「订阅」里填）")
			}
			protocol, label = "http", "HTTP"
			p.Protocol, p.Kind = "http", "http"
			if scheme == "https" {
				fallback = "tls"
			}
		}
		if q.Get("tls") == "1" || q.Get("tls") == "true" {
			fallback = "tls"
		}
		t, err := transport(q, p.Server, fallback)
		if err != nil {
			return p, err
		}
		settings := obj{"address": p.Server, "port": p.Port}
		if name := u.User.Username(); name != "" {
			pass, _ := u.User.Password()
			settings["user"], settings["pass"] = name, pass
		}
		udp := "UDP"
		if protocol == "http" {
			udp = ""
		}
		if t.kind != "tcp" && t.kind != "tls" {
			p.Kind = t.kind
		}
		p.Summary = summary(label, t.label, udp)
		p.Outbound = obj{"tag": "proxy", "protocol": protocol, "settings": settings, "streamSettings": t.stream}
		return p, p.extensions(q, "")
	}
	return p, fmt.Errorf("scheme %q not supported", scheme)
}

// schemes are the share-link prefixes Parse reads.
var schemes = []string{"vless", "hysteria2", "hy2", "trojan", "vmess", "ss", "socks", "socks5", "http", "https"}

// vmessCiphers are the VMess "security" values Xray takes.
var vmessCiphers = []string{"auto", "aes-128-gcm", "chacha20-poly1305", "none", "zero"}

// userinfo is a link's whole user part: "pw", or "a:b" when the secret itself
// has a colon (Hysteria2 userpass auth, Trojan passwords).
func userinfo(u *url.URL) string {
	auth := u.User.Username()
	if pw, ok := u.User.Password(); ok {
		auth += ":" + pw
	}
	return auth
}

func encLabel(encryption string) string {
	if strings.HasPrefix(encryption, "mlkem768x25519plus") {
		return "MLKEM768"
	}
	return ""
}

func summary(parts ...string) string {
	var keep []string
	for _, p := range parts {
		if p != "" {
			keep = append(keep, strings.ToUpper(p))
		}
	}
	return strings.Join(keep, " / ")
}

func or(v, fallback string) string {
	if v == "" {
		return fallback
	}
	return v
}
