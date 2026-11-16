package engine

import (
	"errors"
	"strings"
	"testing"
	"time"
)

func TestNetworkEvent(t *testing.T) {
	wifi, hotspot, none := netState{true, "192.168.1.5"}, netState{true, "192.168.77.146"}, netState{}
	cases := []struct {
		name      string
		prev, cur netState
		slept     time.Duration
		note      string
		stale     bool
	}{
		{"steady", wifi, wifi, 0, "", false},
		{"lost", wifi, none, 0, "network down", false},
		{"still offline", none, none, 0, "", false},
		{"back", none, wifi, 0, "network up (192.168.1.5)", true},
		{"other network", wifi, hotspot, 0, "network changed: 192.168.1.5 → 192.168.77.146", true},
		{"woke online", wifi, wifi, 10 * time.Minute, "woke after 10m0s", true},
		// Awake but still without a network: nothing to reconnect yet.
		{"woke offline", none, none, 10 * time.Minute, "woke after 10m0s", false},
		{"late tick is not a sleep", wifi, wifi, 8 * time.Second, "", false},
	}
	for _, c := range cases {
		note, stale := networkEvent(c.prev, c.cur, c.slept)
		if !strings.HasPrefix(note, c.note) || (c.note == "") != (note == "") || stale != c.stale {
			t.Errorf("%s: got (%q, %v), want (%q…, %v)", c.name, note, stale, c.note, c.stale)
		}
	}
}

// The node counts as down only after repeated misses, the log says so once
// and then every nodeDownRepeat, and connections are ended only after an
// outage long enough to have killed them.
func TestNodeHealth(t *testing.T) {
	var n nodeHealth
	at := time.Unix(1_800_000_000, 0)
	miss := errors.New("timeout")

	if note, stale, wait := n.observe("US-1", 42, nil, at); note != "" || stale || wait != nodeCheckEvery {
		t.Fatalf("healthy check: %q %v %v", note, stale, wait)
	}
	if note, _, wait := n.observe("US-1", 0, miss, at); !strings.Contains(note, "missed a check") || n.down() || wait != nodeCheckWhenFailed {
		t.Fatalf("one miss must not mark the node down: %q", note)
	}
	if note, _, _ := n.observe("US-1", 0, miss, at.Add(15*time.Second)); !strings.Contains(note, "does not answer") || !n.down() {
		t.Fatalf("second miss: %q", note)
	}
	if note, _, _ := n.observe("US-1", 0, miss, at.Add(30*time.Second)); note != "" {
		t.Fatalf("a known outage is not logged again at once: %q", note)
	}
	if note, _, _ := n.observe("US-1", 0, miss, at.Add(15*time.Second+nodeDownRepeat)); !strings.Contains(note, "still does not answer") {
		t.Fatalf("long outage reminder: %q", note)
	}
	note, stale, wait := n.observe("US-1", 180, nil, at.Add(20*time.Minute))
	if !strings.Contains(note, "answers again after") || !stale || n.down() || wait != nodeCheckEvery {
		t.Fatalf("recovery: %q %v", note, stale)
	}

	// A short blip: reported, but TCP rides it out, so connections stay.
	n.observe("US-1", 0, miss, at)
	n.observe("US-1", 0, miss, at.Add(15*time.Second))
	if note, stale, _ := n.observe("US-1", 90, nil, at.Add(30*time.Second)); note == "" || stale {
		t.Fatalf("short outage: %q %v", note, stale)
	}
}
