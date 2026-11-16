package tracker

import (
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"f50/proxy/internal/logtap"
)

// A connection that sent data and heard nothing for this long is reported.
const noReplyAfter = 2 * time.Second

// connError receives what an outbound says went wrong with one connection
// (session.TrackedConnectionError).
type connError struct {
	mu  sync.Mutex
	err error
}

func (c *connError) SubmitError(err error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.err == nil {
		c.err = err
	}
}

func (c *connError) get() error {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.err
}

// connTotals counts ended connections since the last status line.
type connTotals struct {
	ended, failed, noReply atomic.Int64
}

// cause is an outbound error without the wrapper every one of them carries,
// cut to a length a log line can hold.
func cause(err error) string {
	text := strings.TrimPrefix(err.Error(), "failed to process outbound traffic > ")
	if r := []rune(text); len(r) > 400 {
		text = string(r[:400]) + "…"
	}
	return text
}

// endLine describes a connection that ended badly, or "" for one that ended
// as connections do. key groups the reports that say the same thing.
func endLine(c Conn, err error) (line, key string) {
	if c.Outbound == "block" {
		return "", ""
	}
	target := fmt.Sprintf("%s %s:%d", c.Network, c.Host, c.Port)
	if c.IP != "" {
		target += " (" + c.IP + ")"
	}
	via := c.Outbound
	if c.Group != "" {
		via += " [" + c.Group + "]"
	}
	from := c.Inbound
	if c.Protocol != "" {
		from += "/" + c.Protocol
	}
	lived := time.Duration(c.End-c.Start) * time.Millisecond
	tail := fmt.Sprintf("%s via %s from %s after %s, up %d B, down %d B", target, via, from, lived.Round(100*time.Millisecond), c.Up, c.Down)
	switch {
	case err != nil:
		why := cause(err)
		kind := "conn failed"
		if c.Down > 0 {
			// It worked and then broke: the stall the user sees.
			kind = "conn cut off"
		}
		return fmt.Sprintf("%s: %s: %s", kind, tail, why), kind + "|" + via + "|" + c.Host + "|" + why
	case c.Up > 0 && c.Down == 0 && lived >= noReplyAfter:
		return "conn no reply: " + tail, "noreply|" + via + "|" + c.Host
	}
	return "", ""
}

// reportEnd counts an ended connection and logs it when it ended badly.
func (t *Tracker) reportEnd(c Conn, err error) {
	t.totals.ended.Add(1)
	line, key := endLine(c, err)
	if line == "" {
		return
	}
	if err != nil {
		t.totals.failed.Add(1)
	} else {
		t.totals.noReply.Add(1)
	}
	if write, suffix := logtap.AllowTrouble(key, time.Now()); write {
		logtap.Note(line + suffix)
	}
}

// StatusLine sums up the connections since the last call: a heartbeat that
// shows, in the file, when traffic stopped flowing or failures began. The
// caller appends what it knows about the node (engine.statusLine).
func (t *Tracker) StatusLine(since time.Duration, up, down int64) string {
	t.mu.Lock()
	open := len(t.open)
	t.mu.Unlock()
	return fmt.Sprintf("status: %d open; last %s: %d connections ended, %d failed, %d no reply; up +%d B, down +%d B",
		open, since.Round(time.Second), t.totals.ended.Swap(0), t.totals.failed.Swap(0), t.totals.noReply.Swap(0), up, down)
}
