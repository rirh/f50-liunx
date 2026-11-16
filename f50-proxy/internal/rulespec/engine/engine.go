// Package engine routes connections through One Node's rule language
// (pkg/rulespec) with sing-box's matching semantics, for clients that run the
// rules themselves instead of handing them to sing-box. testdata holds
// verdicts computed by sing-box's own matcher; engine_test.go checks them.
package engine

import (
	"fmt"
	"net/netip"

	"f50/proxy/internal/rulespec"
)

// Conn is one connection as the router sees it.
type Conn struct {
	// Domain is the requested or sniffed host name, "" when unknown.
	Domain string
	// IP is the destination address when the target is one.
	IP netip.Addr
	// Resolved holds Domain's addresses once a resolve step looked them up.
	Resolved []netip.Addr
	Port     uint16
	// Network is "tcp" or "udp"; Protocol the sniffed protocol (tls, http,
	// quic, ...) or "".
	Network  string
	Protocol string
}

// Step is one entry of a route: a rule, or a resolve step that looks up the
// domain so later IP conditions can match it, as sing-box's resolve action.
type Step struct {
	Rule    rulespec.Rule
	Resolve bool
}

// RuleSets returns the headless rules of a rule set.
type RuleSets func(tag string) ([]rulespec.Match, error)

// Result is the first rule a connection matched: its step index, action and
// outbound. Matched is false when no rule did.
type Result struct {
	Matched  bool
	Index    int
	Action   string
	Outbound string
}

// Router is a compiled route.
type Router struct {
	steps []compiledStep
}

type compiledStep struct {
	matcher *matcher
	rule    rulespec.Rule
	resolve bool
}

// New compiles steps, loading each rule set once through sets.
func New(steps []Step, sets RuleSets) (*Router, error) {
	c := &compiler{sets: sets, loaded: map[string]*ruleSet{}}
	router := &Router{steps: make([]compiledStep, 0, len(steps))}
	for i, step := range steps {
		if step.Resolve {
			router.steps = append(router.steps, compiledStep{resolve: true})
			continue
		}
		matcher, err := c.compile(step.Rule.Match)
		if err != nil {
			return nil, fmt.Errorf("step %d: %w", i, err)
		}
		router.steps = append(router.steps, compiledStep{matcher: matcher, rule: step.Rule})
	}
	return router, nil
}

// Route returns the first rule conn matches. resolve looks up conn.Domain
// for a resolve step; it is called at most once and may be nil.
func (r *Router) Route(conn *Conn, resolve func() []netip.Addr) Result {
	for i, step := range r.steps {
		if step.resolve {
			if conn.Domain != "" && !conn.IP.IsValid() && conn.Resolved == nil && resolve != nil {
				conn.Resolved = resolve()
				if conn.Resolved == nil {
					conn.Resolved = []netip.Addr{}
				}
			}
			continue
		}
		if step.matcher.match(conn) {
			return Result{Matched: true, Index: i, Action: step.rule.Action, Outbound: step.rule.Outbound}
		}
	}
	return Result{}
}

// Match reports whether conn matches one rule condition, rule sets loaded
// through sets.
func Match(match rulespec.Match, sets RuleSets, conn *Conn) (bool, error) {
	c := &compiler{sets: sets, loaded: map[string]*ruleSet{}}
	matcher, err := c.compile(match)
	if err != nil {
		return false, err
	}
	return matcher.match(conn), nil
}
