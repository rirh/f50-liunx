//go:build !linux

package netmark

import (
	"syscall"
)

// markControl: SO_MARK is Linux only; elsewhere (tests on a workstation)
// sockets go unmarked.
func markControl(int) func(network, address string, c syscall.RawConn) error { return nil }
