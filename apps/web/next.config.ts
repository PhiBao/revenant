import type { NextConfig } from "next";

// In development the API runs on another origin, so proxy to it. A production build is a
// static export that the Rust API serves itself, so it needs no rewrites.
const dev = process.env.NODE_ENV === "development";
const api = process.env.REVENANT_API ?? "http://127.0.0.1:8080";

const config: NextConfig = {
  ...(dev ? {} : { output: "export" as const }),
  trailingSlash: true,
  images: { unoptimized: true },
  ...(dev
    ? {
        async rewrites() {
          return [
            { source: "/api/:path*", destination: `${api}/api/:path*` },
            { source: "/p/:path*", destination: `${api}/p/:path*` },
          ];
        },
      }
    : {}),
};

export default config;
