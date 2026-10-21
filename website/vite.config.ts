import { defineConfig } from 'vite'

import { holocron } from '@holocron.so/vite'
import { cloudflare } from '@cloudflare/vite-plugin'

export default defineConfig({
  plugins: [
    holocron({
      entry: './src/server.tsx',
      pagesDir: './src',
      // Routes list both gpuix.dev and www.gpuix.dev, so pick one for prerendered pages.
      prerenderOrigin: 'https://gpuix.dev',
    }),
    cloudflare({
      viteEnvironment: {
        name: 'rsc',
        childEnvironments: ['ssr'],
      },
    }),
  ],
})
