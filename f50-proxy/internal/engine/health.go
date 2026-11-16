package engine

import (
	"context"
	"fmt"
	"net"
	"sync"
	"sync/atomic"
	"time"

	"f50/proxy/internal/logtap"
	"f50/proxy/internal/platform"
	"f50/proxy/internal/probe"
	"f50/proxy/internal/tracker"
	"f50/proxy/internal/xrayconf"

	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/common/session"
	"github.com/xtls/xray-core/core"
)

// The health watcher tells the three things a dead connection can mean apart
// — the router has no network (no mobile data), the network changed under
// the connection (a new WAN address after the modem re-attached), or the
// node does not answer — and writes each to the log once, so a night without
// signal does not read as hours of node failures.
//
// It also ends the connections a network change killed. Their sockets to the
// node are gone, but nothing tells the clients: the side towards them stays
// open until each client's own timeout. Closing them makes them reconnect at
// once.
const (
	healthTick = 5 * time.Second
	// A tick this late means the machine slept; connections older than the
	// sleep have usually lost their NAT mapping.
	healthSleepGap = 30 * time.Second
	// The node is tested this often, and faster while it fails.
	nodeCheckEvery      = 60 * time.Second
	nodeCheckWhenFailed = 15 * time.Second
	nodeCheckTimeout    = 6 * time.Second
	// Two misses in a row before the node counts as down: one slow answer on a
	// mobile link is not an outage.
	nodeFailLimit = 2
	// After an outage this long the old connections are ended on recovery;
	// shorter ones TCP rides out by itself.
	staleAfterOutage = 45 * time.Second
	// While the node stays down the log repeats it this often.
	nodeDownRepeat = 10 * time.Minute
	// A status line (statusLine) is written this often.
	statusEvery = 5 * time.Minute
)

// Health is what the watcher last saw; part of Status.
type Health struct {
	// Online: the router has a route to the internet.
	Online bool `json:"online"`
	// NodeOK: the last request through the node was answered (true until tested).
	NodeOK  bool  `json:"node_ok"`
	DelayMs int64 `json:"delay_ms,omitempty"`
	// Since is when Online or NodeOK last changed, Unix ms.
	Since int64  `json:"since"`
	Error string `json:"error,omitempty"`
}

type netState struct {
	online bool
	local  string // the address this machine sends from
}

// localRoute is the address the router reaches the internet from. A UDP
// "connect" picks the route without sending a packet.
var localRoute = func() netState {
	for _, target := range []string{"223.5.5.5:53", "[2400:3200::1]:53"} {
		network := "udp4"
		if target[0] == '[' {
			network = "udp6"
		}
		conn, err := net.Dial(network, target)
		if err != nil {
			continue
		}
		local, _, _ := net.SplitHostPort(conn.LocalAddr().String())
		_ = conn.Close()
		if ip := net.ParseIP(local); ip != nil && !ip.IsLoopback() && !ip.IsUnspecified() {
			return netState{online: true, local: local}
		}
	}
	return netState{}
}

// networkEvent compares two looks at the network: what to log, and whether
// the open connections can no longer be alive.
func networkEvent(prev, cur netState, slept time.Duration) (note string, stale bool) {
	switch {
	case prev.online && !cur.online:
		return "network down: the router has no route to the internet", false
	case !prev.online && cur.online:
		return fmt.Sprintf("network up (%s)", cur.local), true
	case cur.online && prev.local != cur.local:
		return fmt.Sprintf("network changed: %s → %s", prev.local, cur.local), true
	case slept >= healthSleepGap:
		return fmt.Sprintf("woke after %s", slept.Round(time.Second)), cur.online
	}
	return "", false
}

type healthWatcher struct {
	stop context.CancelFunc

	mu     sync.Mutex
	health Health
}

func (w *healthWatcher) snapshot() *Health {
	w.mu.Lock()
	defer w.mu.Unlock()
	h := w.health
	return &h
}

func (w *healthWatcher) set(change func(*Health)) {
	w.mu.Lock()
	defer w.mu.Unlock()
	before := w.health
	change(&w.health)
	if before.Online != w.health.Online || before.NodeOK != w.health.NodeOK {
		w.health.Since = time.Now().UnixMilli()
	}
}

// healthSources is what the watcher reads besides the network itself.
type healthSources struct {
	// checkNode times a request through the node; nil when nothing goes
	// through it (直连).
	checkNode func(context.Context) (int64, error)
	// traffic is the run's upload and download so far, for the status line.
	traffic func() (up, down int64)
	// NodeDown follows the node's state for the router (Policy.AppHosts).
	NodeDown *atomic.Bool
}

// watchHealth runs until the returned watcher is stopped.
func watchHealth(node string, tr *tracker.Tracker, src healthSources) *healthWatcher {
	ctx, cancel := context.WithCancel(context.Background())
	first := localRoute()
	w := &healthWatcher{stop: cancel, health: Health{Online: first.online, NodeOK: true, Since: time.Now().UnixMilli()}}
	if first.online {
		logtap.Note("network: sending from " + first.local)
	} else {
		logtap.Warn("network down: the router has no route to the internet")
	}
	go w.run(ctx, first, node, tr, src)
	return w
}

func (w *healthWatcher) run(ctx context.Context, state netState, node string, tr *tracker.Tracker, src healthSources) {
	checkNode := src.checkNode
	ticker := time.NewTicker(healthTick)
	defer ticker.Stop()
	var (
		// Round(0) drops the monotonic reading, which stands still during sleep.
		lastTick  = time.Now().Round(0)
		nextCheck = time.Now().Add(3 * time.Second)
		nodeState nodeHealth

		lastStatus     = time.Now()
		lastUp, lastDn int64
	)
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
		now := time.Now().Round(0)
		slept := now.Sub(lastTick) - healthTick
		lastTick = now

		cur := localRoute()
		if ctx.Err() != nil {
			return
		}
		note, stale := networkEvent(state, cur, slept)
		state = cur
		w.set(func(h *Health) { h.Online = cur.online })
		if note != "" {
			if stale {
				note += fmt.Sprintf(", closed %d stale connections", tr.Close(0))
				nextCheck = time.Now().Add(time.Second)
			}
			logtap.Note(note)
		}
		if time.Since(lastStatus) >= statusEvery {
			var up, down int64
			if src.traffic != nil {
				up, down = src.traffic()
			}
			logtap.Note(statusLine(tr, time.Since(lastStatus), up-lastUp, down-lastDn, w.snapshot()))
			lastStatus, lastUp, lastDn = time.Now(), up, down
		}
		if checkNode == nil || !cur.online || time.Now().Before(nextCheck) {
			continue
		}

		probeCtx, cancel := context.WithTimeout(ctx, nodeCheckTimeout)
		ms, err := checkNode(probeCtx)
		cancel()
		if ctx.Err() != nil {
			return
		}
		note, stale, wait := nodeState.observe(node, ms, err, time.Now())
		nextCheck = time.Now().Add(wait)
		if stale {
			note += fmt.Sprintf(", closed %d stale connections", tr.Close(0))
		}
		if note != "" {
			logtap.Note(note)
		}
		w.set(func(h *Health) {
			switch {
			case err == nil:
				h.NodeOK, h.DelayMs, h.Error = true, ms, ""
			case nodeState.down():
				h.NodeOK, h.DelayMs, h.Error = false, 0, err.Error()
			}
		})
		if down := nodeState.down(); src.NodeDown != nil && src.NodeDown.Swap(down) != down {
			if down {
				logtap.Note("app hosts go direct while the node is down")
			} else {
				logtap.Note("app hosts back through the node")
			}
		}
	}
}

// nodeHealth turns the node checks into what to say about them.
type nodeHealth struct {
	fails     int
	downSince time.Time // zero while the node counts as up
	lastNoted time.Time
}

func (n *nodeHealth) down() bool { return !n.downSince.IsZero() }

// observe takes one check's result: the line to log (if any), whether the
// open connections outlived an outage and should be ended, and how long to
// wait before the next check.
func (n *nodeHealth) observe(node string, ms int64, err error, now time.Time) (note string, stale bool, wait time.Duration) {
	if err == nil {
		if n.down() {
			outage := now.Sub(n.downSince)
			note = fmt.Sprintf("node %q answers again after %s (%d ms)", node, outage.Round(time.Second), ms)
			stale = outage >= staleAfterOutage
		}
		*n = nodeHealth{}
		return note, stale, nodeCheckEvery
	}
	n.fails++
	switch {
	case n.fails < nodeFailLimit:
		note = fmt.Sprintf("node %q missed a check (%v)", node, err)
	case !n.down():
		n.downSince, n.lastNoted = now, now
		note = fmt.Sprintf("node %q does not answer (%v); the network itself is up", node, err)
	case now.Sub(n.lastNoted) >= nodeDownRepeat:
		n.lastNoted = now
		note = fmt.Sprintf("node %q still does not answer after %s (%v)", node, now.Sub(n.downSince).Round(time.Second), err)
	}
	return note, false, nodeCheckWhenFailed
}

// proxyProbe times one request through the instance's "proxy" outbound, the
// way a group's delay test goes through its node (xrayconf.ProbeInbound).
func proxyProbe(inst *core.Instance) func(context.Context) (int64, error) {
	return func(ctx context.Context) (int64, error) {
		ctx = session.ContextWithInbound(ctx, &session.Inbound{Tag: xrayconf.ProbeInbound + "proxy", CanSpliceCopy: 3})
		return probe.HTTPDelay(ctx, func(ctx context.Context, dest xnet.Destination) (net.Conn, error) {
			return core.Dial(ctx, inst, dest)
		}, "")
	}
}

// statusLine is the tracker's heartbeat with the node's state, the trouble
// lines the cap held back and the process's memory.
func statusLine(t *tracker.Tracker, since time.Duration, up, down int64, h *Health) string {
	line := t.StatusLine(since, up, down)
	switch {
	case h == nil:
	case !h.Online:
		line += "; network down"
	case !h.NodeOK:
		line += "; node not answering (" + h.Error + ")"
	case h.DelayMs > 0:
		line += fmt.Sprintf("; node %d ms", h.DelayMs)
	}
	if n := logtap.DroppedTrouble(); n > 0 {
		line += fmt.Sprintf("; %d trouble lines not written (over %d a minute)", n, logtap.TroublePerMinute)
	}
	return line + fmt.Sprintf("; mem %d MB", platform.MemBytes()>>20)
}
