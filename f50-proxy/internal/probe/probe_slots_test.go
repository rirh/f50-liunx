package probe

import (
	"context"
	"testing"
	"time"
)

// A URL probe waiting for a turn gives up at its deadline instead of
// starting another Xray instance.
func TestURLWaitsForATurn(t *testing.T) {
	for range MaxURLProbes {
		urlSlots <- struct{}{}
	}
	defer func() {
		for range MaxURLProbes {
			<-urlSlots
		}
	}()
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	start := time.Now()
	if _, err := URL(ctx, "not a link", nil, ""); err == nil || err.Error() != "timeout" {
		t.Fatalf("err = %v, want timeout", err)
	}
	if time.Since(start) < 40*time.Millisecond {
		t.Fatal("did not wait for a turn")
	}
}
