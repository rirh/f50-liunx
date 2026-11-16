package engine

import (
	"fmt"
	"net/netip"
	"slices"
	"sort"
)

// ipSet holds CIDRs as sorted, merged address ranges for binary search;
// geoip sets reach hundreds of thousands of prefixes.
type ipSet struct {
	ranges []ipRange
}

type ipRange struct{ from, to netip.Addr }

func newIPSet(cidrs []string) (*ipSet, error) {
	ranges := make([]ipRange, 0, len(cidrs))
	for _, cidr := range cidrs {
		prefix, err := netip.ParsePrefix(cidr)
		if err != nil {
			address, addrErr := netip.ParseAddr(cidr)
			if addrErr != nil {
				return nil, fmt.Errorf("ip_cidr %q: %w", cidr, err)
			}
			prefix = netip.PrefixFrom(address, address.BitLen())
		}
		prefix = prefix.Masked()
		ranges = append(ranges, ipRange{prefix.Addr(), lastAddr(prefix)})
	}
	slices.SortFunc(ranges, func(a, b ipRange) int { return a.from.Compare(b.from) })
	merged := ranges[:0]
	for _, current := range ranges {
		if n := len(merged); n > 0 && merged[n-1].from.Is4() == current.from.Is4() && current.from.Compare(merged[n-1].to) <= 0 {
			if current.to.Compare(merged[n-1].to) > 0 {
				merged[n-1].to = current.to
			}
			continue
		}
		merged = append(merged, current)
	}
	return &ipSet{ranges: merged}, nil
}

func (s *ipSet) contains(address netip.Addr) bool {
	// Ranges hold plain IPv4 and IPv6 addresses, as sing-box's IP sets do.
	address = address.Unmap()
	i := sort.Search(len(s.ranges), func(i int) bool { return s.ranges[i].to.Compare(address) >= 0 })
	return i < len(s.ranges) && s.ranges[i].from.Compare(address) <= 0
}

func lastAddr(prefix netip.Prefix) netip.Addr {
	raw := prefix.Addr().AsSlice()
	for bit := prefix.Bits(); bit < len(raw)*8; bit++ {
		raw[bit/8] |= 0x80 >> uint(bit%8)
	}
	address, _ := netip.AddrFromSlice(raw)
	return address
}
