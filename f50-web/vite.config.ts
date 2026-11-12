import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";

const headers = {
  "Content-Security-Policy": "frame-ancestors 'none'",
  "X-Frame-Options": "DENY",
};

// The device the dev server talks to (its uhttpd: /ubus JSON-RPC and /cgi-bin/cgi-upload). F50_DEVICE overrides it.
const device = process.env.F50_DEVICE ?? "http://192.168.77.1";

export default defineConfig(() => {
  // Evaluated by Vite at build time, not when a visitor opens the page.
  const buildTime = new Date().toISOString();

  return {
    // served by the device's uhttpd from /www/f50
    base: "/f50/",
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
    },
    define: {
      "import.meta.env.VITE_BUILD_TIME": JSON.stringify(buildTime),
    },
    server: {
      host: "127.0.0.1",
      port: 5173,
      strictPort: true,
      open: false,
      headers,
      proxy: {
        "/ubus": { target: device, changeOrigin: true },
        "/cgi-bin": { target: device, changeOrigin: true },
      },
    },
    preview: { headers },
  };
});
