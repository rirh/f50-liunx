// Package api is the token-protected loopback HTTP API (/v1/*) f50d drives the
// engine through.
package api

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"strconv"
	"strings"
	"time"

	"f50/proxy/internal/engine"
	"f50/proxy/internal/link"
	"f50/proxy/internal/logtap"
	"f50/proxy/internal/probe"
	"f50/proxy/internal/route"
	"f50/proxy/internal/xrayconf"
)

type Server struct {
	token    string
	engine   *engine.Engine
	shutdown func()
}

// New serves the engine's control API; token is the Bearer token every request
// must carry, shutdown what POST /v1/shutdown runs.
func New(token string, engine *engine.Engine, shutdown func()) *Server {
	return &Server{token: token, engine: engine, shutdown: shutdown}
}

// Handler is the API's routes, behind the token check.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /v1/status", func(w http.ResponseWriter, r *http.Request) {
		reply(w, s.engine.Status(), nil)
	})
	mux.HandleFunc("POST /v1/start", func(w http.ResponseWriter, r *http.Request) {
		var req engine.StartRequest
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		log.Printf("api: start %q (mode %s, routing %s, port %d, tproxy port %d)", req.Name, req.Mode, req.Routing, req.Port, req.TProxyPort)
		st, err := s.engine.Start(req)
		reply(w, st, err)
	})
	mux.HandleFunc("POST /v1/stop", func(w http.ResponseWriter, r *http.Request) {
		if s.engine.Status().Running {
			log.Print("api: stop")
		}
		reply(w, s.engine.Stop(), nil)
	})
	mux.HandleFunc("GET /v1/connections", func(w http.ResponseWriter, r *http.Request) {
		reply(w, s.engine.Tracker().Connections(), nil)
	})
	mux.HandleFunc("POST /v1/connections/close", func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			ID uint64 `json:"id"` // 0: all
		}
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		reply(w, map[string]int{"closed": s.engine.Tracker().Close(req.ID)}, nil)
	})
	mux.HandleFunc("GET /v1/logs", func(w http.ResponseWriter, r *http.Request) {
		after, _ := strconv.ParseUint(r.URL.Query().Get("after"), 10, 64)
		reply(w, logtap.Since(r.URL.Query().Get("epoch"), after), nil)
	})
	mux.HandleFunc("POST /v1/probe", func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Link   string   `json:"link"`
			Via    []string `json:"via"`
			Method string   `json:"method"`
			URL    string   `json:"url"`
			// 1–30 s; anything else means 5 s.
			TimeoutMs int64 `json:"timeout_ms"`
		}
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		timeout := 5 * time.Second
		if req.TimeoutMs >= 1000 && req.TimeoutMs <= 30000 {
			timeout = time.Duration(req.TimeoutMs) * time.Millisecond
		}
		ctx, cancel := context.WithTimeout(r.Context(), timeout)
		defer cancel()
		ms, err := probe.Node(ctx, req.Link, probe.Options{Method: req.Method, URL: req.URL, Via: req.Via})
		reply(w, map[string]int64{"ms": ms}, err)
	})
	mux.HandleFunc("POST /v1/subscription", func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			URL  string `json:"url"`
			Text string `json:"text"`
		}
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		sub := subscription{Body: req.Text}
		if req.URL != "" {
			fetched, err := s.fetch(r.Context(), req.URL)
			if err != nil {
				reply(w, nil, err)
				return
			}
			sub = fetched
		}
		nodes := link.ParseSubscription(sub.Body)
		if len(nodes) == 0 {
			reply(w, nil, fmt.Errorf("没有找到节点链接"))
			return
		}
		reply(w, map[string]any{"nodes": nodes, "title": sub.Title, "usage": sub.Usage}, nil)
	})
	// f50d checks the 配置 tab's JSON here before saving it: counts and
	// warnings, or the first error. The 规则 policy compiles too, so what saves
	// also starts.
	mux.HandleFunc("POST /v1/rules/check", func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Text     string   `json:"text"`
			RuleSets string   `json:"rule_sets"`
			Nodes    []string `json:"nodes"` // node names outbounds and groups may use
		}
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		rt, err := route.CompileRoute(req.Text, route.CatalogueAt(req.RuleSets), req.Nodes...)
		if err == nil {
			_, err = route.NewPolicy(xrayconf.RoutingRule, rt)
		}
		reply(w, rt, err)
	})
	// A 配置's link: f50d checks the text it returns before keeping it.
	mux.HandleFunc("POST /v1/rules/fetch", func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			URL string `json:"url"`
		}
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		text, err := s.fetchRules(r.Context(), strings.TrimSpace(req.URL))
		reply(w, map[string]string{"text": text}, err)
	})
	// 策略组 of the running route.
	mux.HandleFunc("GET /v1/groups", func(w http.ResponseWriter, r *http.Request) {
		groups, err := s.engine.Groups()
		reply(w, groups, err)
	})
	mux.HandleFunc("POST /v1/groups/select", func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Group string `json:"group"`
			Node  string `json:"node"`
		}
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		groups, err := s.engine.SelectGroup(req.Group, req.Node)
		reply(w, groups, err)
	})
	mux.HandleFunc("POST /v1/groups/test", func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Group string `json:"group"`
		}
		if err := decode(r, &req); err != nil {
			reply(w, nil, err)
			return
		}
		groups, err := s.engine.TestGroup(r.Context(), req.Group)
		reply(w, groups, err)
	})
	mux.HandleFunc("POST /v1/shutdown", func(w http.ResponseWriter, r *http.Request) {
		reply(w, map[string]bool{"ok": true}, nil)
		go s.shutdown()
	})
	return s.auth(mux)
}

func (s *Server) auth(next http.Handler) http.Handler {
	want := []byte("Bearer " + s.token)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Loopback only, but any local process or web page could reach the port.
		if subtle.ConstantTimeCompare([]byte(r.Header.Get("Authorization")), want) != 1 {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		next.ServeHTTP(w, r)
	})
}

func decode(r *http.Request, v any) error {
	return json.NewDecoder(io.LimitReader(r.Body, 1<<20)).Decode(v)
}

// reply writes {"ok":true,"data":...} or {"ok":false,"error":"..."}.
func reply(w http.ResponseWriter, data any, err error) {
	status, payload := http.StatusOK, map[string]any{"ok": true, "data": data}
	if err != nil {
		status, payload = http.StatusUnprocessableEntity, map[string]any{"ok": false, "error": err.Error()}
	}
	// A fixed length (never chunked) keeps f50d's loopback client trivial.
	body, _ := json.Marshal(payload)
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Content-Length", strconv.Itoa(len(body)))
	w.WriteHeader(status)
	_, _ = w.Write(body)
}
