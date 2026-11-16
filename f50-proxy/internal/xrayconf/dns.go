package xrayconf

import "f50/proxy/internal/geodata"

// DNS. Names are resolved inside the core, never by the system resolver, so
// lookups don't leak to the local network's DNS: mainland names (规则) or
// all names (直连) through a mainland DoH server, direct; everything else
// through a DoH server reached over the node. The router sends each server's
// queries by their inbound tag (internal/router).
const (
	DNSRemoteTag = "dns-remote"
	DNSDirectTag = "dns-direct"
	DNSOutTag    = "dns-out"

	// How long past its TTL a cached answer may still be served (dnsSection).
	dnsStaleSeconds = 3600

	dnsRemote = "https://1.1.1.1/dns-query"
	dnsDirect = "https://223.5.5.5/dns-query"
	// tproxyTag is the gateway's transparent inbound: DNS to port 53 that
	// reaches it is answered by the core (router.fixedRoute).
	TProxyTag = "tproxy"
)

// DNSSummary says in a line what dnsSection sets up, for the log.
func DNSSummary(routing string) string {
	switch routing {
	case RoutingDirect:
		return dnsDirect + " direct"
	case RoutingRule:
		return dnsRemote + " through the node, mainland names " + dnsDirect + " direct"
	}
	return dnsRemote + " through the node"
}

func dnsSection(mode, routing string) Obj {
	remote := Obj{"address": dnsRemote, "tag": DNSRemoteTag}
	direct := Obj{"address": dnsDirect, "tag": DNSDirectTag}
	var servers []Obj
	switch routing {
	case RoutingDirect:
		servers = []Obj{direct}
	case RoutingRule:
		direct["domains"] = geodata.CNDomains()
		direct["skipFallback"] = true
		servers = []Obj{remote, direct}
	default:
		servers = []Obj{remote}
	}
	strategy := "UseIP"
	if mode == ModeGateway {
		// AAAA queries get an empty answer, so LAN clients (whose DNS dns-out
		// answers) connect over IPv4, and direct dials stay on IPv4 too.
		strategy = "UseIPv4"
	}
	return Obj{
		"servers": servers, "queryStrategy": strategy, "useSystemHosts": true,
		// A name looked up before is answered from the cache even once its TTL
		// ran out (for dnsStaleSeconds more) while a fresh answer is fetched
		// behind it. Otherwise every lookup that meets a slow moment of the node
		// holds its connection for the 4 s timeout before routing can decide.
		"serveStale": true, "serveExpiredTTL": dnsStaleSeconds,
	}
}

// AppHostsDNS has the client's own hosts (Policy.AppHosts) always looked up
// through the mainland DoH server, direct: they leave direct while the node
// is down, and a lookup through a dead node would fail them first. The
// answer is the same either way (they sit behind Cloudflare's anycast).
func AppHostsDNS(doc Obj, routing string, hosts []string) {
	if len(hosts) == 0 || routing == RoutingDirect {
		return
	}
	dns := doc["dns"].(Obj)
	servers := dns["servers"].([]Obj)
	names := make([]string, 0, len(hosts))
	for _, h := range hosts {
		names = append(names, "domain:"+h)
	}
	for _, s := range servers {
		if s["tag"] == DNSDirectTag {
			own, _ := s["domains"].([]string)
			// geodata.CNDomains is shared: never append to it in place.
			s["domains"] = append(names, own...)
			return
		}
	}
	dns["servers"] = append(servers, Obj{"address": dnsDirect, "tag": DNSDirectTag, "domains": names, "skipFallback": true})
}
