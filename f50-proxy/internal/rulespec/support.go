package rulespec

import (
	"slices"
	"strings"
)

// Client is a profile format the language renders to.
type Client string

const (
	SingBox Client = "singbox"
	Mihomo  Client = "mihomo"
)

// Field is one match field: its sing-box key, admin label and the clients
// that can express it. Logical rules and invert work in every client.
type Field struct {
	Key     string
	Label   string
	Clients []Client
}

var everyClient = []Client{SingBox, Mihomo}

// Fields is the language's support table; docs/routing-rules.md mirrors it.
var Fields = []Field{
	{Key: "domain", Label: "完整域名", Clients: everyClient},
	{Key: "domain_suffix", Label: "域名后缀", Clients: everyClient},
	{Key: "domain_keyword", Label: "域名关键字", Clients: everyClient},
	{Key: "domain_regex", Label: "域名正则", Clients: everyClient},
	{Key: "ip_cidr", Label: "IP 段", Clients: everyClient},
	{Key: "ip_is_private", Label: "私有 IP", Clients: everyClient},
	{Key: "port", Label: "端口", Clients: everyClient},
	{Key: "port_range", Label: "端口范围", Clients: everyClient},
	{Key: "network", Label: "网络", Clients: everyClient},
	// mihomo matches no sniffed protocol.
	{Key: "protocol", Label: "嗅探协议", Clients: []Client{SingBox}},
	{Key: "rule_set", Label: "规则集", Clients: everyClient},
}

// Label returns the admin label of a field key, or the key itself.
func Label(key string) string {
	for _, field := range Fields {
		if field.Key == key {
			return field.Label
		}
	}
	return key
}

// Unsupported lists the fields of m, nested rules included, that client
// cannot express. A client must skip such a rule as a whole: dropping one
// condition of it would widen what the rule matches.
func Unsupported(m Match, client Client) []string {
	var result []string
	add := func(key string) {
		if !slices.Contains(result, key) {
			result = append(result, key)
		}
	}
	var walk func(Match)
	walk = func(current Match) {
		for _, key := range current.fields() {
			for _, field := range Fields {
				if field.Key == key && !slices.Contains(field.Clients, client) {
					add(key)
				}
			}
		}
		// mihomo rule lines are comma separated with parenthesised logic.
		if client == Mihomo && slices.ContainsFunc(current.DomainRegex, func(pattern string) bool {
			return strings.ContainsAny(pattern, ",()")
		}) {
			add("domain_regex")
		}
		for _, nested := range current.Rules {
			walk(nested)
		}
	}
	walk(m)
	return result
}

// fields lists the match fields a default rule sets, in table order.
func (m Match) fields() []string {
	set := map[string]bool{
		"domain": len(m.Domain) > 0, "domain_suffix": len(m.DomainSuffix) > 0,
		"domain_keyword": len(m.DomainKeyword) > 0, "domain_regex": len(m.DomainRegex) > 0,
		"ip_cidr": len(m.IPCIDR) > 0, "ip_is_private": m.IPIsPrivate,
		"port": len(m.Port) > 0, "port_range": len(m.PortRange) > 0,
		"network": len(m.Network) > 0, "protocol": len(m.Protocol) > 0, "rule_set": len(m.RuleSet) > 0,
	}
	var result []string
	for _, field := range Fields {
		if set[field.Key] {
			result = append(result, field.Key)
		}
	}
	return result
}
