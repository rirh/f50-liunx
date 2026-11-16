package tracker

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"

	xnet "github.com/xtls/xray-core/common/net"
)

func TestSourceIP(t *testing.T) {
	for _, c := range []struct {
		src  xnet.Destination
		want string
	}{
		{xnet.TCPDestination(xnet.ParseAddress("192.168.0.23"), 50123), "192.168.0.23"},
		{xnet.UDPDestination(xnet.ParseAddress("::ffff:192.168.0.23"), 5353), "192.168.0.23"},
		{xnet.TCPDestination(xnet.ParseAddress("fd00::23"), 443), "fd00::23"},
		{xnet.TCPDestination(xnet.ParseAddress("example.com"), 443), ""},
		{xnet.Destination{}, ""},
	} {
		if got := sourceIP(c.src); got != c.want {
			t.Errorf("%v: %q, want %q", c.src, got, c.want)
		}
	}
}

func TestEndLine(t *testing.T) {
	c := Conn{Start: 1000, End: 5200, Network: "tcp", Host: "api.example.com", IP: "1.2.3.4", Port: 443,
		Protocol: "tls", Inbound: "tproxy", Outbound: "proxy", Up: 517, Source: "192.168.0.10"}
	reset := errors.New("failed to process outbound traffic > proxy/vless/outbound: connection ends > read: connection reset by peer")

	line, key := endLine(c, reset)
	for _, want := range []string{"conn failed", "tcp api.example.com:443 (1.2.3.4)", "via proxy", "from tproxy/tls", "after 4.2s", "up 517 B, down 0 B", "connection reset by peer"} {
		if !strings.Contains(line, want) {
			t.Errorf("%q lacks %q", line, want)
		}
	}
	if strings.Contains(line, "failed to process outbound traffic") || key == "" {
		t.Errorf("wrapper kept, or no key: %q", line)
	}

	// Data had flowed: this is the stall, named apart.
	c.Down = 88 << 20
	if line, _ := endLine(c, reset); !strings.HasPrefix(line, "conn cut off") {
		t.Errorf("cut-off connection: %q", line)
	}
	if line, _ := endLine(c, nil); line != "" {
		t.Errorf("a normal end is not logged: %q", line)
	}

	// A pooled keep-alive connection: a request answered, then silence until
	// the idle timeout ends it with an error. Nobody was waiting on it.
	idle := c
	idle.Down, idle.Last, idle.End = 4862, c.Start+800, c.Start+300_100
	if line, _ := endLine(idle, reset); line != "" {
		t.Errorf("an idle connection's end is not a cut-off: %q", line)
	}
	// The same error while data still flowed is the stall.
	idle.Last = idle.End - 500
	if line, _ := endLine(idle, reset); !strings.HasPrefix(line, "conn cut off") {
		t.Errorf("cut off while busy: %q", line)
	}

	c.Down = 0
	if line, _ := endLine(c, nil); !strings.HasPrefix(line, "conn no reply") {
		t.Errorf("sent, nothing back: %q", line)
	}
	c.End = c.Start + 300
	if line, _ := endLine(c, nil); line != "" {
		t.Errorf("too short to call it no reply: %q", line)
	}
	c.Outbound = "block"
	if line, _ := endLine(c, reset); line != "" {
		t.Errorf("blocked on purpose: %q", line)
	}
}

// A fresh core has nothing open or closed; the 数据 page reads both as lists.
func TestConnectionsEmptyListsAreArrays(t *testing.T) {
	raw, err := json.Marshal(New().Connections())
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{`"active":[]`, `"closed":[]`} {
		if !strings.Contains(string(raw), want) {
			t.Errorf("want %s in %s", want, raw)
		}
	}
}
