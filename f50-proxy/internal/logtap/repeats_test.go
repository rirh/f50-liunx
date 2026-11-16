package logtap

import (
	"strings"
	"testing"
	"time"
)

func TestRepeatsFoldAndCap(t *testing.T) {
	var r repeats
	at := time.Unix(1_800_000_000, 0)
	if write, suffix := r.allow("a", at); !write || suffix != "" {
		t.Fatalf("first line: %v %q", write, suffix)
	}
	for i := range 5 {
		if write, _ := r.allow("a", at.Add(time.Duration(i+1)*time.Second)); write {
			t.Fatal("a repeat inside the window is only counted")
		}
	}
	write, suffix := r.allow("a", at.Add(repeatWindow))
	if !write || !strings.Contains(suffix, "+5 more") {
		t.Fatalf("after the window: %v %q", write, suffix)
	}

	// A flood of different lines stops at the minute's budget and is counted.
	var flood repeats
	written := 0
	for i := range TroublePerMinute + 25 {
		if write, _ := flood.allow(string(rune('A'+i%26))+strings.Repeat("x", i), at); write {
			written++
		}
	}
	if written != TroublePerMinute || flood.dropped() != 25 || flood.dropped() != 0 {
		t.Fatalf("budget: wrote %d", written)
	}
	if write, _ := flood.allow("next minute", at.Add(time.Minute)); !write {
		t.Fatal("the budget renews each minute")
	}
}

func TestWorthFiling(t *testing.T) {
	for text, want := range map[string]bool{
		"[123] proxy/dokodemo: received request for 192.168.0.10:50000":                      false,
		"[123] transport/internet: failed to dial to 1.2.3.4:443":                            true,
		"[123] app/proxyman/outbound: failed to process outbound traffic":                    false, // reportEnd writes it
		"[123] proxy/socks: TCP Connect request to tcp:example.com:443":                      false,
		"app/dispatcher: default route for tcp:example.com:443":                              false,
		"app/dns: failed to lookup ip for domain e2e.test at server DOH//1.1.1.1 > rcode: 3": false,
	} {
		if worthFiling(text) != want {
			t.Errorf("worthFiling(%q) = %v", text, !want)
		}
	}
	if repeatKey("[4159279002] proxy/http: failed") != "proxy/http: failed" {
		t.Error("session id kept in the repeat key")
	}
}
