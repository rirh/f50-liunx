.PHONY: dev open

dev:
	"$(PYTHON)" scripts/dev/luci-dev.py --device "$(DEVICE)" --port "$(PORT)"

open:
	"$(PYTHON)" -m webbrowser -t "http://127.0.0.1:$(PORT)/"
