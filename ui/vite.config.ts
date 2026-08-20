import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  server: { port: 5173, strictPort: true },
  build: { target: "safari15" },
  clearScreen: false,
});
