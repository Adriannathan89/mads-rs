import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  poweredByHeader: false,
  experimental: {
    useTypeScriptCli: false,
  },
  turbopack: {
    root: process.cwd(),
  },
};

export default nextConfig;
