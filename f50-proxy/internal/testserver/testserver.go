// Package testserver is a throwaway One Node on loopback for trying the engine
// without real nodes (f50-proxy testserver, and the engine's tests).
package testserver

import (
	"crypto/ecdh"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/base64"
	"fmt"
	"io"
	"log"
	"math/big"
	"net"
	"net/http"
	"net/url"
	"os"
	"time"

	"f50/proxy/internal/xrayconf"

	"github.com/xtls/xray-core/core"
)

type obj = xrayconf.Obj

// Run is a throwaway One Node on loopback for checking the client
// without real nodes: a VLESS REALITY server (Vision over TCP, and XHTTP) whose
// camouflage site and "internet" are local. It prints
//
//	TARGET http://e2e.test:<port>/hello   (only reachable through the server)
//	LINK   vless://...#reality-vision
//	LINK   vless://...#reality-xhttp
//
// preceded by PLINK lines: the same "internet" behind plain Shadowsocks (AEAD and
// 2022), SOCKS5, HTTP, VMess and Trojan inbounds, for the other protocols.
//
// It must run in its own process: a REALITY server and client in one Go process
// stall after the handshake (see node one/dataplane/xray/xray_test.go).
func Run() {
	target := listenTarget()
	dest := listenCamouflage("e2e.test")
	k, err := ecdh.X25519().GenerateKey(rand.Reader)
	if err != nil {
		fatal(err)
	}
	priv := base64.RawURLEncoding.EncodeToString(k.Bytes())
	pub := base64.RawURLEncoding.EncodeToString(k.PublicKey().Bytes())
	const uuid = "27a4f0d6-7f1a-4c55-9b0a-0d6f7c1b2e3a"

	vision, xhttp := FreeTCPPort(), FreeTCPPort()
	ss, ss2022, socks, httpProxy, vmess, trojan := FreeTCPPort(), FreeTCPPort(), FreeTCPPort(), FreeTCPPort(), FreeTCPPort(), FreeTCPPort()
	const key2022 = "MDEyMzQ1Njc4OWFiY2RlZg==" // 16 bytes, 2022-blake3-aes-128-gcm
	reality := func(network string) obj {
		s := obj{
			"network": network, "security": "reality",
			"realitySettings": obj{
				"target": dest, "serverNames": []string{"e2e.test"},
				"privateKey": priv, "shortIds": []string{"0123abcd"},
			},
		}
		if network == "xhttp" {
			s["xhttpSettings"] = obj{"path": "/e2e", "mode": "auto"}
		}
		return s
	}
	doc := obj{
		"log": obj{"loglevel": "error"},
		"dns": obj{"hosts": obj{"e2e.test": "127.0.0.1"}},
		"inbounds": []obj{
			{
				"listen": "127.0.0.1", "port": vision, "protocol": "vless",
				"settings":       obj{"clients": []obj{{"id": uuid, "flow": "xtls-rprx-vision"}}, "decryption": "none"},
				"streamSettings": reality("raw"),
			},
			{
				"listen": "127.0.0.1", "port": xhttp, "protocol": "vless",
				"settings":       obj{"clients": []obj{{"id": uuid}}, "decryption": "none"},
				"streamSettings": reality("xhttp"),
			},
			{"listen": "127.0.0.1", "port": ss, "protocol": "shadowsocks", "settings": obj{"method": "aes-256-gcm", "password": "pw", "network": "tcp,udp"}},
			{"listen": "127.0.0.1", "port": ss2022, "protocol": "shadowsocks", "settings": obj{"method": "2022-blake3-aes-128-gcm", "password": key2022, "network": "tcp,udp"}},
			{"listen": "127.0.0.1", "port": socks, "protocol": "socks", "settings": obj{"auth": "password", "accounts": []obj{{"user": "u", "pass": "p@ss"}}, "udp": true}},
			{"listen": "127.0.0.1", "port": httpProxy, "protocol": "http", "settings": obj{"accounts": []obj{{"user": "u", "pass": "p"}}}},
			{"listen": "127.0.0.1", "port": vmess, "protocol": "vmess", "settings": obj{"clients": []obj{{"id": uuid}}}},
			{"listen": "127.0.0.1", "port": trojan, "protocol": "trojan", "settings": obj{"clients": []obj{{"password": "p:w"}}}},
		},
		// Server inbounds blackhole private targets by default; the target is on loopback.
		"outbounds": []obj{{"protocol": "freedom", "settings": obj{
			"targetStrategy": "UseIP",
			"finalRules":     []obj{{"action": "allow", "ip": []string{"127.0.0.0/8"}}},
		}}},
	}
	config, err := xrayconf.Build(doc)
	if err != nil {
		fatal(err)
	}
	inst, err := core.New(config)
	if err != nil {
		fatal(err)
	}
	if err := inst.Start(); err != nil {
		fatal(err)
	}
	common := fmt.Sprintf("security=reality&sni=e2e.test&fp=chrome&pbk=%s&sid=0123abcd", pub)
	fmt.Printf("TARGET http://e2e.test:%d/hello\n", target)
	fmt.Printf("PLINK ss://%s@127.0.0.1:%d#ss\n", base64.RawURLEncoding.EncodeToString([]byte("aes-256-gcm:pw")), ss)
	fmt.Printf("PLINK ss://2022-blake3-aes-128-gcm:%s@127.0.0.1:%d#ss2022\n", url.QueryEscape(key2022), ss2022)
	fmt.Printf("PLINK socks5://u:p%%40ss@127.0.0.1:%d#socks5\n", socks)
	fmt.Printf("PLINK http://u:p@127.0.0.1:%d#http\n", httpProxy)
	fmt.Printf("PLINK vmess://%s\n", base64.StdEncoding.EncodeToString(fmt.Appendf(nil, `{"v":"2","ps":"vmess","add":"127.0.0.1","port":%d,"id":"%s","aid":"0","net":"tcp","tls":""}`, vmess, uuid)))
	fmt.Printf("PLINK trojan://p%%3Aw@127.0.0.1:%d?security=none#trojan\n", trojan)
	fmt.Printf("LINK vless://%s@127.0.0.1:%d?%s&type=tcp&flow=xtls-rprx-vision#reality-vision\n", uuid, vision, common)
	fmt.Printf("LINK vless://%s@127.0.0.1:%d?%s&type=xhttp&path=%%2Fe2e&mode=auto#reality-xhttp\n", uuid, xhttp, common)
	select {}
}

// listenTarget serves /hello and /generate_204 (so the delay probe can be
// pointed here too).
func listenTarget() int {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		fatal(err)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/hello", func(w http.ResponseWriter, r *http.Request) { io.WriteString(w, "hello through the node") })
	mux.HandleFunc("/generate_204", func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(http.StatusNoContent) })
	go http.Serve(ln, mux)
	return ln.Addr().(*net.TCPAddr).Port
}

// listenCamouflage is the REALITY target: any TLS 1.3 server.
func listenCamouflage(name string) string {
	key, _ := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	tmpl := &x509.Certificate{
		SerialNumber: big.NewInt(1), Subject: pkix.Name{CommonName: name}, DNSNames: []string{name},
		NotBefore: time.Now().Add(-time.Hour), NotAfter: time.Now().Add(24 * time.Hour),
	}
	der, _ := x509.CreateCertificate(rand.Reader, tmpl, tmpl, &key.PublicKey, key)
	ln, err := tls.Listen("tcp", "127.0.0.1:0", &tls.Config{
		Certificates: []tls.Certificate{{Certificate: [][]byte{der}, PrivateKey: key}},
		MinVersion:   tls.VersionTLS13, NextProtos: []string{"h2", "http/1.1"},
	})
	if err != nil {
		fatal(err)
	}
	srv := &http.Server{
		Handler:  http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { io.WriteString(w, "camouflage") }),
		ErrorLog: log.New(io.Discard, "", 0), // REALITY probes look like bad handshakes
	}
	go srv.Serve(ln)
	return ln.Addr().String()
}

func FreeTCPPort() int {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		fatal(err)
	}
	defer ln.Close()
	return ln.Addr().(*net.TCPAddr).Port
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "f50-proxy testserver:", err)
	os.Exit(1)
}
