import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createMDX } from 'fumadocs-mdx/next';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const withMDX = createMDX();

/** @type {import('next').NextConfig} */
const config = {
  reactStrictMode: true,
  output: 'export',
  outputFileTracingRoot: path.join(__dirname, '..'),
  transpilePackages: [],
  webpack: (config) => {
    config.resolve.symlinks = false;
    return config;
  },
};

export default withMDX(config);
