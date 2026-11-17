// Package version is the engine's release version and the User-Agent strings
// built from it.
package version

import "runtime"

// Version is the release version, stamped by the Makefile via
// -ldflags "-X f50/proxy/internal/version.Version=…".
var Version = "dev"

// ClientUA names this engine to rule catalogues and 配置 hosts, in the shape of
// One Node PC's ("OneNodePC/<ver> <arch>"): the gateway records the
// architecture token with the device.
var ClientUA = "F50Proxy/" + Version + " " + runtime.GOARCH

// SubscriptionUA makes subscription panels answer with base64 share links:
// they negotiate by the first product token, and nearly all know v2rayN. The
// One Node gateway shows the F50Proxy token as the client name and version.
var SubscriptionUA = "v2rayN/7.15 " + ClientUA
