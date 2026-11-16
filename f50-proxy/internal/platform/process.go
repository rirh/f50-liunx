// Package platform reads what the engine needs from the OS: its memory, whether
// it runs as root, and whether its parent process is alive.
package platform

import (
	"errors"
	"os"
	"syscall"
	"time"
)

func IsElevated() bool { return os.Geteuid() == 0 }

// WaitParent polls with signal 0.
func WaitParent(pid int) {
	for {
		if err := syscall.Kill(pid, 0); errors.Is(err, syscall.ESRCH) {
			return
		}
		time.Sleep(time.Second)
	}
}
