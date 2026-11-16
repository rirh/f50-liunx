package link

import (
	"encoding/json"
	"net/url"
	"strconv"
	"strings"
)

// Share-link normalizers: the non-standard forms clients export, rewritten
// into the standard links Parse reads.

// normalizeLink rewrites the non-standard share-link forms clients export into
// the standard ones the rest of the parser reads (see each normalizer).
// Anything else is returned unchanged.
func normalizeLink(link string) string {
	scheme, _, _ := strings.Cut(link, "://")
	switch strings.ToLower(scheme) {
	case "vless":
		return normalizeVLESS(link)
	case "vmess":
		return normalizeVMess(link)
	case "ss":
		return normalizeShadowsocks(link)
	case "socks", "socks5":
		return normalizeSocks(link)
	}
	return link
}

// normalizeVLESS rewrites Shadowrocket's VLESS export form into the standard
// one the rest of the parser reads:
//
//	vless://base64(<encryption>:<uuid>@<host>:<port>)?remarks=…&tls=1&peer=<sni>
//	  &xtls=2&pbk=…&sid=…&fingerprint=…&obfs=xhttp&obfsParam=<host>&path=…&mode=…
func normalizeVLESS(link string) string {
	_, rest, _ := strings.Cut(link, "://")
	head, tail := rest, ""
	if i := strings.IndexAny(rest, "?#"); i >= 0 {
		head, tail = rest[:i], rest[i:]
	}
	if strings.Contains(head, "@") {
		return link
	}
	decoded, ok := decodeBase64(head)
	at := strings.LastIndex(decoded, "@")
	if !ok || at < 0 {
		return link
	}
	cred, hostport := decoded[:at], decoded[at+1:]
	colon := strings.LastIndex(cred, ":")
	if colon < 0 {
		return link
	}
	encryption, uuid := cred[:colon], cred[colon+1:]
	in, err := url.Parse("vless://x@" + hostport + tail)
	if err != nil {
		return link
	}
	q := in.Query()
	out := url.Values{}
	if encryption == "" || encryption == "auto" {
		encryption = "none"
	}
	out.Set("encryption", encryption)
	if q.Get("tls") == "1" {
		out.Set("security", "tls")
		if q.Get("pbk") != "" {
			out.Set("security", "reality")
		}
	}
	for to, from := range map[string]string{"sni": "peer", "fp": "fingerprint", "pbk": "pbk", "sid": "sid", "spx": "spx", "alpn": "alpn"} {
		if v := q.Get(from); v != "" {
			out.Set(to, v)
		}
	}
	if q.Get("xtls") == "2" {
		out.Set("flow", "xtls-rprx-vision")
	}
	switch obfs := q.Get("obfs"); obfs {
	case "", "none":
		out.Set("type", "tcp")
	case "xhttp":
		out.Set("type", "xhttp")
		out.Set("path", q.Get("path"))
		if h := q.Get("obfsParam"); h != "" {
			out.Set("host", h)
		}
		if m := q.Get("mode"); m != "" {
			out.Set("mode", m)
		}
	case "websocket":
		out.Set("type", "ws")
		out.Set("path", q.Get("path"))
		if h := q.Get("obfsParam"); h != "" {
			out.Set("host", h)
		}
	case "grpc":
		out.Set("type", "grpc")
		out.Set("serviceName", q.Get("path"))
	default:
		out.Set("type", obfs) // anything else is reported as unsupported
	}
	name := in.Fragment
	if name == "" {
		name = q.Get("remarks")
	}
	return (&url.URL{Scheme: "vless", User: url.User(uuid), Host: hostport, RawQuery: out.Encode(), Fragment: name}).String()
}

// normalizeVMess turns v2rayN's base64 JSON form into the standard VMess link
// (Xray's share-link proposal), which Parse reads:
//
//	vmess://base64({"ps":…,"add":…,"port":…,"id":…,"aid":…,"scy":…,"net":…,
//	  "type":…,"host":…,"path":…,"tls":…,"sni":…,"alpn":…,"fp":…})
//	→ vmess://<id>@<add>:<port>?encryption=<scy>&type=<net>&security=<tls>&…#<ps>
func normalizeVMess(link string) string {
	_, rest, _ := strings.Cut(link, "://")
	if strings.Contains(rest, "@") {
		return link
	}
	body, name, _ := strings.Cut(rest, "#")
	decoded, ok := decodeBase64(body)
	if !ok {
		return link
	}
	var v map[string]any
	if json.Unmarshal([]byte(decoded), &v) != nil {
		return link
	}
	field := func(k string) string {
		switch x := v[k].(type) {
		case string:
			return strings.TrimSpace(x)
		case float64:
			return strconv.FormatFloat(x, 'f', -1, 64)
		}
		return ""
	}
	out := url.Values{}
	set := func(k, val string) {
		if val != "" {
			out.Set(k, val)
		}
	}
	network := strings.ToLower(or(field("net"), "tcp"))
	set("encryption", or(field("scy"), "auto"))
	set("type", network)
	set("security", or(field("tls"), "none"))
	set("sni", field("sni"))
	set("alpn", field("alpn"))
	set("fp", field("fp"))
	if aid := field("aid"); aid != "0" {
		set("aid", aid)
	}
	switch network {
	case "grpc":
		set("serviceName", field("path"))
		set("mode", field("type"))
		set("authority", field("authority"))
	case "tcp", "raw":
		if t := field("type"); t != "" && t != "none" {
			set("headerType", t)
		}
	default:
		set("path", field("path"))
		set("host", field("host"))
		if network == "xhttp" || network == "splithttp" {
			set("mode", field("type"))
		}
	}
	if name == "" {
		name = field("ps")
	} else if n, err := url.PathUnescape(name); err == nil {
		name = n
	}
	host := field("add")
	if strings.Contains(host, ":") {
		host = "[" + host + "]"
	}
	return (&url.URL{Scheme: "vmess", User: url.User(field("id")), Host: host + ":" + field("port"), RawQuery: out.Encode(), Fragment: name}).String()
}

// normalizeShadowsocks turns SIP002's base64 userinfo and the legacy
// all-base64 form into the plain one the subgateway renders:
//
//	ss://base64(<method>:<password>)@<host>:<port>?plugin=…#<name>
//	ss://base64(<method>:<password>@<host>:<port>)#<name>
//	→ ss://<method>:<password>@<host>:<port>#<name> (percent-encoded)
func normalizeShadowsocks(link string) string {
	_, rest, _ := strings.Cut(link, "://")
	head, tail := rest, ""
	if i := strings.IndexAny(rest, "?#"); i >= 0 {
		head, tail = rest[:i], rest[i:]
	}
	head = strings.TrimSuffix(head, "/")
	at := strings.LastIndex(head, "@")
	cred, hostport := "", ""
	if at < 0 {
		decoded, ok := decodeBase64(head)
		at = strings.LastIndex(decoded, "@")
		if !ok || at < 0 {
			return link
		}
		cred, hostport = decoded[:at], decoded[at+1:]
	} else {
		cred, hostport = head[:at], head[at+1:]
		if unescaped, err := url.PathUnescape(cred); err == nil {
			cred = unescaped
		}
		if !strings.Contains(cred, ":") {
			decoded, ok := decodeBase64(cred)
			if !ok {
				return link
			}
			cred = decoded
		}
	}
	method, password, ok := strings.Cut(cred, ":")
	if !ok {
		return link
	}
	in, err := url.Parse("ss://x@" + hostport + tail)
	if err != nil {
		return link
	}
	in.User = url.UserPassword(method, password)
	return in.String()
}

// normalizeSocks decodes the base64 userinfo v2rayN and Shadowrocket write
// (socks://base64(<user>:<pass>)@<host>:<port>) into the plain form.
func normalizeSocks(link string) string {
	u, err := url.Parse(link)
	if err != nil || u.User == nil {
		return link
	}
	if _, has := u.User.Password(); has {
		return link
	}
	decoded, ok := decodeBase64(u.User.Username())
	user, pass, found := strings.Cut(decoded, ":")
	if !ok || !found {
		return link
	}
	u.User = url.UserPassword(user, pass)
	return u.String()
}
