import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Static export: the output is embedded into the Rust binary by rust-embed
  // and served by axum on the same origin as the API.
  output: "export",
  devIndicators: false,
  images: { unoptimized: true },
};

export default nextConfig;
