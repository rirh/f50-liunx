package route

import (
	"fmt"
	"strings"
	"sync/atomic"

	"f50/proxy/internal/geodata"
	"f50/proxy/internal/rulespec"
	"f50/proxy/internal/rulespec/engine"
	"f50/proxy/internal/xrayconf"
)

// Policy is what the router runs: the compiled steps and the outbound for
// connections no rule matched. Resolve, when set, turns a group's tag into
// its current node's.
type Policy struct {
	Router  *engine.Router
	Final   string
	Resolve func(tag string) string
	// AppHosts are the client's own sites (its window, updates, rule sets).
	// While the node does not answer they go direct, so a dead node never
	// takes away the window the user would fix it from (see xrayconf.AppHostsDNS).
	AppHosts []string
	// NodeDown is set by the health watcher while the node counts as down.
	NodeDown atomic.Bool
}

// AppHost is true for one of AppHosts or a name under it.
func (p *Policy) AppHost(domain string) bool {
	for _, h := range p.AppHosts {
		if domain == h || strings.HasSuffix(domain, "."+h) {
			return true
		}
	}
	return false
}

// Tags of the built-in lists the 规则 mode tail uses, apart from any set the
// 配置 tab defines under a public name.
const (
	mainlandDomainsTag = "one-node:mainland-domains"
	overseasDomainsTag = "one-node:overseas-domains"
	mainlandIPsTag     = "one-node:mainland-ips"
)

// localSuffixes are names only the local network can resolve.
var localSuffixes = []string{"local", "lan", "localdomain", "home.arpa", "internal"}

// ruleSets serves the route's sets and the built-in tail lists to the engine.
func (r Route) ruleSets(tag string) ([]rulespec.Match, error) {
	switch tag {
	case mainlandDomainsTag:
		return []rulespec.Match{geodata.MainlandDomains()}, nil
	case overseasDomainsTag:
		return geodata.OverseasDomains(), nil
	case mainlandIPsTag:
		return []rulespec.Match{{IPCIDR: geodata.CNIPs()}}, nil
	}
	if rules, ok := r.Sets[tag]; ok {
		return rules, nil
	}
	return nil, fmt.Errorf("规则集 %s 没有加载", tag)
}

// Policy is what each routing mode runs. Private ranges and local names stay
// direct everywhere; 全局 and 直连 send everything else one way, as Clash and
// Shadowrocket do. 规则 follows sing-box's usual China setup: the route, then
// mainland domains direct and, when the final is the node, sites known to be
// overseas to it, so only names on neither list are resolved (through the
// node, see Document's DNS) for the mainland IP list.
func NewPolicy(routing string, route Route) (*Policy, error) {
	direct := func(match rulespec.Match) engine.Step {
		return engine.Step{Rule: rulespec.Rule{Match: match, Action: rulespec.ActionRoute, Outbound: "direct"}}
	}
	steps := []engine.Step{
		direct(rulespec.Match{IPCIDR: geodata.PrivateNets}),
		direct(rulespec.Match{Domain: []string{"localhost"}, DomainSuffix: localSuffixes}),
	}
	final := "proxy"
	routeFinal := route.Final
	if route.finalTag != "" {
		routeFinal = route.finalTag
	}
	switch routing {
	case xrayconf.RoutingDirect:
		final = "direct"
	case xrayconf.RoutingRule:
		steps = append(steps, route.Steps...)
		steps = append(steps, direct(rulespec.Match{RuleSet: []string{mainlandDomainsTag}}))
		if routeFinal != "direct" && routeFinal != "block" {
			steps = append(steps, engine.Step{Rule: rulespec.Rule{
				Match: rulespec.Match{RuleSet: []string{overseasDomainsTag}}, Action: rulespec.ActionRoute, Outbound: routeFinal,
			}})
		}
		steps = append(steps,
			engine.Step{Resolve: true},
			direct(rulespec.Match{RuleSet: []string{mainlandIPsTag}}),
		)
		final = routeFinal
	}
	router, err := engine.New(steps, route.ruleSets)
	if err != nil {
		return nil, err
	}
	return &Policy{Router: router, Final: final}, nil
}
