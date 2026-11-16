// Package rulespec is One Node's routing-rule language: a strict subset of
// sing-box route rules with sing-box's JSON field names and matching
// semantics, so a rule renders to sing-box as it is and translates to other
// clients. docs/routing-rules.md describes the language.
//
// https://sing-box.sagernet.org/configuration/route/rule/
package rulespec

// Rule types, logical modes and actions, spelled as in sing-box.
const (
	TypeLogical = "logical"
	ModeAnd     = "and"
	ModeOr      = "or"

	ActionRoute  = "route"
	ActionReject = "reject"
)

// Limits keep one rule small enough for every client.
const (
	// MaxValues caps the match values of one rule, nested rules included.
	MaxValues = 500
	// MaxDepth allows a logical rule and two nested levels below it.
	MaxDepth = 3
	// MaxRegexLength caps one domain_regex pattern.
	MaxRegexLength = 256
)

// Networks and Protocols are the accepted network and sniffed-protocol values.
var (
	Networks  = []string{"tcp", "udp"}
	Protocols = []string{"http", "tls", "quic", "stun", "dns", "bittorrent", "dtls", "ssh", "rdp", "ntp"}
)

// Match is the condition of a route rule: a default rule (Type "") or a
// logical rule (Type TypeLogical, with Mode and Rules). Values are
// normalized and de-duplicated by the JSON decoder.
//
// A default rule follows sing-box: the address fields (domain,
// domain_suffix, domain_keyword, domain_regex, ip_cidr, ip_is_private and
// the rule sets) match when any of them matches, the port fields likewise,
// and those groups and every other field must all match.
type Match struct {
	Type string

	Domain []string
	// DomainSuffix "x.com" matches x.com and its subdomains; ".x.com" only
	// the subdomains.
	DomainSuffix  []string
	DomainKeyword []string
	DomainRegex   []string
	// IPCIDR matches the destination IP; for a domain destination only after
	// the profile resolved it.
	IPCIDR      []string
	IPIsPrivate bool
	Port        []uint16
	// PortRange items are "first:last", ":last" or "first:".
	PortRange []string
	Network   []string
	Protocol  []string
	// RuleSet names address rule sets (geosite-*, geoip-*).
	RuleSet []string

	Mode  string
	Rules []Match

	Invert bool
}

// Rule is a route rule: a match and its action. Outbound names the policy
// target of ActionRoute; the caller decides which names exist.
type Rule struct {
	Match
	Action   string
	Outbound string
}

// IsLogical reports whether m combines nested rules.
func (m Match) IsLogical() bool { return m.Type == TypeLogical }

// RuleSets lists the rule-set tags m references, nested rules included.
func (m Match) RuleSets() []string {
	seen := map[string]bool{}
	var tags []string
	var walk func(Match)
	walk = func(current Match) {
		for _, tag := range current.RuleSet {
			if !seen[tag] {
				seen[tag] = true
				tags = append(tags, tag)
			}
		}
		for _, nested := range current.Rules {
			walk(nested)
		}
	}
	walk(m)
	return tags
}

func (m Match) valueCount() int {
	count := len(m.Domain) + len(m.DomainSuffix) + len(m.DomainKeyword) + len(m.DomainRegex) +
		len(m.IPCIDR) + len(m.Port) + len(m.PortRange) + len(m.Network) + len(m.Protocol) + len(m.RuleSet)
	if m.IPIsPrivate {
		count++
	}
	for _, nested := range m.Rules {
		count += nested.valueCount()
	}
	return count
}

func (m Match) empty() bool {
	return !m.IsLogical() && m.valueCount() == 0
}
