package engine

import (
	"bufio"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"strings"
	"testing"
	"time"

	"f50/proxy/internal/route"
	"f50/proxy/internal/testserver"
)

func startTestServer(t *testing.T) (target string, links []string) {
	t.Helper()
	target, links, _ = startTestServerAll(t)
	return target, links
}

// startTestServerAll also returns the plain-protocol links (PLINK).
func startTestServerAll(t *testing.T) (target string, links, others []string) {
	t.Helper()
	cmd := exec.Command(os.Args[0], "-test.run=^$")
	cmd.Env = append(os.Environ(), "F50PROXY_TESTSERVER=1")
	cmd.Stderr = os.Stderr
	out, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = cmd.Process.Kill(); _ = cmd.Wait() })
	sc := bufio.NewScanner(out)
	for len(links) < 2 && sc.Scan() {
		line := sc.Text()
		if v, ok := strings.CutPrefix(line, "TARGET "); ok {
			target = v
		}
		if v, ok := strings.CutPrefix(line, "LINK "); ok {
			links = append(links, v)
		}
		if v, ok := strings.CutPrefix(line, "PLINK "); ok {
			others = append(others, v)
		}
	}
	if target == "" || len(links) != 2 {
		t.Fatal("test server did not print its links")
	}
	go io.Copy(io.Discard, out)
	return target, links, others
}

// TestMain doubles as the test server process.
func TestMain(m *testing.M) {
	if os.Getenv("F50PROXY_TESTSERVER") == "1" {
		testserver.Run()
	}
	// The device's rule-set cache is not the test machine's to write.
	route.CacheDir = ""
	os.Exit(m.Run())
}

func get(t *testing.T, scheme string, port int, target string) string {
	t.Helper()
	proxy := &url.URL{Scheme: scheme, Host: fmt.Sprintf("127.0.0.1:%d", port)}
	client := &http.Client{Timeout: 10 * time.Second, Transport: &http.Transport{Proxy: http.ProxyURL(proxy)}}
	resp, err := client.Get(target)
	if err != nil {
		t.Fatalf("%s: %v", scheme, err)
	}
	defer resp.Body.Close()
	b, _ := io.ReadAll(resp.Body)
	return string(b)
}

// fetchVia gets a Cloudflare trace through the mixed port: "<egress ip> <loc>".
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
