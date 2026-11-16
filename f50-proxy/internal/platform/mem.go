package platform

import (
	"runtime/metrics"
)

var memSamples = []metrics.Sample{
	{Name: "/memory/classes/total:bytes"},
	{Name: "/memory/classes/heap/released:bytes"},
}

// MemBytes is the memory this process holds from the OS (what the Go runtime
// mapped, minus heap it handed back): close to its resident size, as the core
// has no cgo. runtime/metrics reads it without stopping the world.
func MemBytes() uint64 {
	s := make([]metrics.Sample, len(memSamples))
	copy(s, memSamples)
	metrics.Read(s)
	total, released := s[0].Value.Uint64(), s[1].Value.Uint64()
	if released > total {
		return 0
	}
	return total - released
}
