// Command f50-proxy is the F50's proxy engine: Xray-core in a long-running
// procd service, driven by f50d over a token-protected loopback API. It is
// One Node's PC core (onecore) cut down to Linux, with a gateway mode for the
// router's transparent proxy (DESIGN.md §1).
//
//	f50-proxy serve  --listen 127.0.0.1:17891 --token-file f [--log f] [--loglevel l] [--cache-dir d] [--parent-pid n] [--info-stdout]
//	f50-proxy check  --links file [--mode proxy] [--port 17890]
//	f50-proxy testserver   (loopback REALITY node for trying the engine, see internal/testserver)
//	f50-proxy version
//
// serve prints "F50PROXY_LISTEN <addr>" once the API is up. The token comes
// from --token-file (written by f50d) or F50PROXY_TOKEN.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"runtime"
	"strings"
	"syscall"
	"time"

	"f50/proxy/internal/api"
	"f50/proxy/internal/dailylog"
	"f50/proxy/internal/engine"
	"f50/proxy/internal/link"
	"f50/proxy/internal/logtap"
	"f50/proxy/internal/platform"
	"f50/proxy/internal/probe"
	"f50/proxy/internal/route"
	"f50/proxy/internal/testserver"
	"f50/proxy/internal/version"
	"f50/proxy/internal/xrayconf"

	"github.com/xtls/xray-core/core"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: f50-proxy serve|check|testserver|version")
		os.Exit(2)
	}
	switch os.Args[1] {
	case "serve":
		serve(os.Args[2:])
	case "check":
		check(os.Args[2:])
	case "testserver":
		testserver.Run()
	case "version":
		fmt.Printf("f50-proxy %s xray=%s %s/%s\n", version.Version, core.Version(), runtime.GOOS, runtime.GOARCH)
	default:
		fmt.Fprintln(os.Stderr, "unknown command", os.Args[1])
		os.Exit(2)
	}
}

func serve(args []string) {
	fs := flag.NewFlagSet("serve", flag.ExitOnError)
	listen := fs.String("listen", "127.0.0.1:0", "control API address (loopback)")
	tokenFile := fs.String("token-file", "", "read the API token from this file instead of F50PROXY_TOKEN")
	parentPID := fs.Int("parent-pid", 0, "exit when this process is gone")
	logPath := fs.String("log", "", "core and Xray logs: dir/core.log writes dir/core-<date>.log, last 7 days kept")
	logLevel := fs.String("loglevel", "warning", "Xray log level")
	infoStdout := fs.Bool("info-stdout", false, "log lines that are not trouble on stdout, trouble on stderr (procd: daemon.info / daemon.err)")
	fs.StringVar(&route.CacheDir, "cache-dir", route.CacheDir, "remote rule sets are kept here (empty: memory only)")
	_ = fs.Parse(args)

	// Xray's lines reach the same logger through the tap (internal/logtap).
	log.SetFlags(log.LstdFlags | log.Lmicroseconds)
	var info, trouble io.Writer = os.Stderr, os.Stderr
	if *infoStdout {
		info = os.Stdout
	}
	if *logPath != "" {
		f, err := dailylog.Open(*logPath)
		if err != nil {
			fatal(err)
		}
		info, trouble = io.MultiWriter(info, f), io.MultiWriter(trouble, f)
	}
	log.SetOutput(info)
	logtap.SetTroubleOutput(trouble)
	token := os.Getenv("F50PROXY_TOKEN")
	if *tokenFile != "" {
		raw, err := os.ReadFile(*tokenFile)
		if err != nil {
			fatal(err)
		}
		token = strings.TrimSpace(string(raw))
	}
	if len(token) < 16 {
		fatal(errors.New("missing API token (F50PROXY_TOKEN or --token-file)"))
	}
	host, _, err := net.SplitHostPort(*listen)
	if err != nil || !net.ParseIP(host).IsLoopback() {
		fatal(errors.New("--listen must be a loopback address"))
	}
	ln, err := net.Listen("tcp", *listen)
	if err != nil {
		fatal(err)
	}

	eng := engine.New(*logLevel)
	quit := make(chan string, 1)
	control := api.New(token, eng, func() { quit <- "shutdown request" })
	srv := &http.Server{
		Handler:           control.Handler(),
		ReadHeaderTimeout: 5 * time.Second,
	}
	go func() {
		if err := srv.Serve(ln); err != nil && !errors.Is(err, http.ErrServerClosed) {
			quit <- "api: " + err.Error()
		}
	}()
	if *parentPID > 0 {
		go func() {
			platform.WaitParent(*parentPID)
			quit <- fmt.Sprintf("parent %d exited", *parentPID)
		}()
	}
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt, syscall.SIGTERM)
	go func() { quit <- "signal " + (<-sig).String() }()

	log.Printf("f50-proxy %s %s/%s xray %s elevated=%v pid=%d", version.Version, runtime.GOOS, runtime.GOARCH, core.Version(), platform.IsElevated(), os.Getpid())
	fmt.Printf("F50PROXY_LISTEN %s\n", ln.Addr())

	reason := <-quit
	log.Printf("stopping: %s", reason)
	// Close Xray's listeners and connections before the process dies.
	eng.Stop()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	_ = srv.Shutdown(ctx)
}

// check starts every link from a file in turn and fetches through the mixed port.
// It drives the same engine f50d does, from the command line.
func check(args []string) {
	fs := flag.NewFlagSet("check", flag.ExitOnError)
	links := fs.String("links", "links.local.txt", "share links or a base64 subscription")
	mode := fs.String("mode", xrayconf.ModeProxy, "proxy or gateway")
	routing := fs.String("routing", xrayconf.RoutingGlobal, "global, rule or direct")
	port := fs.Int("port", 17890, "mixed port")
	logLevel := fs.String("loglevel", "warning", "Xray log level")
	fs.StringVar(&link.BindInterface, "interface", "", "bind node connections to this interface (e.g. wan)")
	_ = fs.Parse(args)

	raw, err := os.ReadFile(*links)
	if err != nil {
		fatal(err)
	}
	eng := engine.New(*logLevel)
	failed := false
	for _, node := range link.ParseSubscription(string(raw)) {
		label := node.Kind + "/" + node.Protocol
		if node.Problem != "" {
			fmt.Printf("skip  %-14s %s\n", label, node.Problem)
			continue
		}
		ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
		ms, perr := probe.Delay(ctx, node.Link)
		cancel()
		if _, err := eng.Start(engine.StartRequest{Link: node.Link, Name: node.Name, Mode: *mode, Routing: *routing, Port: *port}); err != nil {
			fmt.Printf("FAIL  %-14s start: %v\n", label, err)
			failed = true
			continue
		}
		trace, terr := fetchVia(*port, "https://www.cloudflare.com/cdn-cgi/trace")
		speed := ""
		if terr == nil {
			speed = downloadVia(*port, "https://speed.cloudflare.com/__down?bytes=20000000")
		}
		st := eng.Stop()
		switch {
		case terr != nil:
			fmt.Printf("FAIL  %-14s %-10s probe=%s fetch: %v\n", label, node.Name, msOrErr(ms, perr), terr)
			failed = true
		default:
			fmt.Printf("ok    %-14s %-10s probe=%s egress %s  %s  (up %d B, down %d B)\n",
				label, node.Name, msOrErr(ms, perr), trace, speed, st.Up, st.Down)
		}
	}
	if failed {
		os.Exit(1)
	}
}

func fetchVia(port int, target string) (string, error) {
	tr := &http.Transport{Proxy: http.ProxyURL(&url.URL{Scheme: "http", Host: fmt.Sprintf("127.0.0.1:%d", port)})}
	client := &http.Client{Timeout: 15 * time.Second, Transport: tr}
	resp, err := client.Get(target)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	var ip, loc string
	for line := range strings.SplitSeq(string(body), "\n") {
		if v, ok := strings.CutPrefix(line, "ip="); ok {
			ip = v
		}
		if v, ok := strings.CutPrefix(line, "loc="); ok {
			loc = v
		}
	}
	return ip + " " + loc, nil
}

// downloadVia times a 20 MB download through the mixed port.
func downloadVia(port int, target string) string {
	tr := &http.Transport{Proxy: http.ProxyURL(&url.URL{Scheme: "http", Host: fmt.Sprintf("127.0.0.1:%d", port)})}
	client := &http.Client{Timeout: 60 * time.Second, Transport: tr}
	start := time.Now()
	resp, err := client.Get(target)
	if err != nil {
		return "download: " + err.Error()
	}
	defer resp.Body.Close()
	n, err := io.Copy(io.Discard, resp.Body)
	secs := time.Since(start).Seconds()
	if err != nil {
		return fmt.Sprintf("download: %v after %d B", err, n)
	}
	return fmt.Sprintf("%.1f Mbps", float64(n)*8/secs/1e6)
}

func msOrErr(ms int64, err error) string {
	if err != nil {
		return "err(" + err.Error() + ")"
	}
	return fmt.Sprintf("%dms", ms)
}

func fatal(err error) {
	fmt.Fprintln(os.Stderr, "f50-proxy:", err)
	os.Exit(1)
}
