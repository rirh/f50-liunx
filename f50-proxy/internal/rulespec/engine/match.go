package engine

import (
	"fmt"
	"net/netip"
	"regexp"
	"slices"
	"strconv"
	"strings"

	"f50/proxy/internal/rulespec"
)

// item tests one condition of a default rule.
type item func(*Conn) bool

// matcher is a compiled default or logical rule. A default rule follows
// sing-box: the address items match when any of them does, the port items
// likewise, and those groups and every other item must all match.
type matcher struct {
	logical bool
	and     bool
	rules   []*matcher

	address []item
	port    []item
	other   []item
	sets    []*ruleSet

	invert bool
}

// ruleSet is a loaded rule set. One plain default rule merges into the
// referencing rule's groups (merge); anything else matches as a whole.
type ruleSet struct {
	merge *matcher
	rules []*matcher
}

type compiler struct {
	sets   RuleSets
	loaded map[string]*ruleSet
}

func (c *compiler) compile(m rulespec.Match) (*matcher, error) {
	out := &matcher{invert: m.Invert}
	if m.IsLogical() {
		out.logical, out.and = true, m.Mode == rulespec.ModeAnd
		for _, nested := range m.Rules {
			compiled, err := c.compile(nested)
			if err != nil {
				return nil, err
			}
			out.rules = append(out.rules, compiled)
		}
		return out, nil
	}
	if len(m.Domain) > 0 || len(m.DomainSuffix) > 0 {
		out.address = append(out.address, domainItem(m.Domain, m.DomainSuffix))
	}
	if len(m.DomainKeyword) > 0 {
		keywords := m.DomainKeyword
		out.address = append(out.address, func(conn *Conn) bool {
			host := strings.ToLower(conn.Domain)
			return host != "" && slices.ContainsFunc(keywords, func(keyword string) bool { return strings.Contains(host, keyword) })
		})
	}
	if len(m.DomainRegex) > 0 {
		patterns := make([]*regexp.Regexp, 0, len(m.DomainRegex))
		for _, value := range m.DomainRegex {
			pattern, err := regexp.Compile(value)
			if err != nil {
				return nil, fmt.Errorf("domain_regex %q: %w", value, err)
			}
			patterns = append(patterns, pattern)
		}
		out.address = append(out.address, func(conn *Conn) bool {
			host := strings.ToLower(conn.Domain)
			return host != "" && slices.ContainsFunc(patterns, func(pattern *regexp.Regexp) bool { return pattern.MatchString(host) })
		})
	}
	if len(m.IPCIDR) > 0 {
		set, err := newIPSet(m.IPCIDR)
		if err != nil {
			return nil, err
		}
		out.address = append(out.address, destinationIP(set.contains))
	}
	if m.IPIsPrivate {
		out.address = append(out.address, destinationIP(func(address netip.Addr) bool { return !isPublic(address) }))
	}
	if len(m.Port) > 0 {
		ports := m.Port
		out.port = append(out.port, func(conn *Conn) bool { return slices.Contains(ports, conn.Port) })
	}
	if len(m.PortRange) > 0 {
		ranges, err := portRanges(m.PortRange)
		if err != nil {
			return nil, err
		}
		out.port = append(out.port, func(conn *Conn) bool {
			return slices.ContainsFunc(ranges, func(r [2]uint16) bool { return conn.Port >= r[0] && conn.Port <= r[1] })
		})
	}
	if len(m.Network) > 0 {
		networks := m.Network
		out.other = append(out.other, func(conn *Conn) bool { return slices.Contains(networks, conn.Network) })
	}
	if len(m.Protocol) > 0 {
		protocols := m.Protocol
		out.other = append(out.other, func(conn *Conn) bool { return slices.Contains(protocols, conn.Protocol) })
	}
	for _, tag := range m.RuleSet {
		set, err := c.ruleSet(tag)
		if err != nil {
			return nil, err
		}
		out.sets = append(out.sets, set)
	}
	return out, nil
}

func (c *compiler) ruleSet(tag string) (*ruleSet, error) {
	if set, ok := c.loaded[tag]; ok {
		return set, nil
	}
	if c.sets == nil {
		return nil, fmt.Errorf("rule set %s: no source", tag)
	}
	rules, err := c.sets(tag)
	if err != nil {
		return nil, fmt.Errorf("rule set %s: %w", tag, err)
	}
	set := &ruleSet{}
	for _, rule := range rules {
		compiled, err := c.compile(rule)
		if err != nil {
			return nil, fmt.Errorf("rule set %s: %w", tag, err)
		}
		set.rules = append(set.rules, compiled)
	}
	if len(set.rules) == 1 && !set.rules[0].logical && !set.rules[0].invert && len(set.rules[0].sets) == 0 {
		set.merge = set.rules[0]
	}
	c.loaded[tag] = set
	return set, nil
}

func (m *matcher) match(conn *Conn) bool {
	var matched bool
	switch {
	case m.logical && m.and:
		matched = !slices.ContainsFunc(m.rules, func(rule *matcher) bool { return !rule.match(conn) })
	case m.logical:
		matched = slices.ContainsFunc(m.rules, func(rule *matcher) bool { return rule.match(conn) })
	default:
		matched = m.matchDefault(conn)
	}
	return matched != m.invert
}

func (m *matcher) matchDefault(conn *Conn) bool {
	if !all(m.other, conn) {
		return false
	}
	if len(m.sets) == 0 {
		return groups(conn, m.address, m.port)
	}
	// Rule sets are alternatives; a mergeable one lends its conditions to
	// this rule's groups, any other must match on top of them.
	for _, set := range m.sets {
		if set.merge != nil {
			if all(set.merge.other, conn) && groups(conn, append(slices.Clip(m.address), set.merge.address...), append(slices.Clip(m.port), set.merge.port...)) {
				return true
			}
			continue
		}
		if groups(conn, m.address, m.port) && slices.ContainsFunc(set.rules, func(rule *matcher) bool { return rule.match(conn) }) {
			return true
		}
	}
	return false
}

// groups reports whether each non-empty group has a matching item.
func groups(conn *Conn, address, port []item) bool {
	return (len(address) == 0 || anyItem(address, conn)) && (len(port) == 0 || anyItem(port, conn))
}

func all(items []item, conn *Conn) bool {
	return !slices.ContainsFunc(items, func(it item) bool { return !it(conn) })
}

func anyItem(items []item, conn *Conn) bool {
	return slices.ContainsFunc(items, func(it item) bool { return it(conn) })
}

// destinationIP matches the destination address, or for a domain the
// addresses a resolve step found; an unresolved domain matches nothing.
func destinationIP(test func(netip.Addr) bool) item {
	return func(conn *Conn) bool {
		if conn.IP.IsValid() {
			return test(conn.IP)
		}
		return slices.ContainsFunc(conn.Resolved, test)
	}
}

// domainItem matches full domains and suffixes: "x.com" covers x.com and
// its subdomains, ".x.com" only the subdomains.
func domainItem(domains, suffixes []string) item {
	full := map[string]bool{}
	for _, domain := range domains {
		full[strings.ToLower(domain)] = true
	}
	withSelf, subdomains := map[string]bool{}, map[string]bool{}
	for _, suffix := range suffixes {
		suffix = strings.ToLower(suffix)
		if name, ok := strings.CutPrefix(suffix, "."); ok {
			subdomains[name] = true
		} else {
			withSelf[suffix] = true
		}
	}
	return func(conn *Conn) bool {
		host := strings.ToLower(conn.Domain)
		if host == "" {
			return false
		}
		if full[host] || withSelf[host] {
			return true
		}
		for name := host; ; {
			dot := strings.IndexByte(name, '.')
			if dot < 0 {
				return false
			}
			name = name[dot+1:]
			if withSelf[name] || subdomains[name] {
				return true
			}
		}
	}
}

func portRanges(values []string) ([][2]uint16, error) {
	ranges := make([][2]uint16, 0, len(values))
	for _, value := range values {
		first, last, ok := strings.Cut(value, ":")
		if !ok {
			return nil, fmt.Errorf("port_range %q", value)
		}
		bounds := [2]uint16{0, 0xFFFF}
		for i, text := range []string{first, last} {
			if text == "" {
				continue
			}
			port, err := strconv.ParseUint(text, 10, 16)
			if err != nil {
				return nil, fmt.Errorf("port_range %q", value)
			}
			bounds[i] = uint16(port)
		}
		ranges = append(ranges, bounds)
	}
	return ranges, nil
}

// isPublic is sing-box's N.IsPublicAddr.
func isPublic(address netip.Addr) bool {
	return !(address.IsPrivate() || address.IsLoopback() || address.IsMulticast() || address.IsLinkLocalUnicast() ||
		address.IsInterfaceLocalMulticast() || address.IsUnspecified())
}
