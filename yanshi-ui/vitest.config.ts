import { defineConfig } from 'vitest/config';

// W5.5: minimal config for the 6 existing *.test.ts(x) unit suites.
// `node` environment is sufficient — none of them render into a DOM
// (ErrorBoundary.test.tsx exercises the class lifecycle methods
// directly, not via a render tree). Kept separate from vite.config.ts
// so the build config stays untouched by test-only concerns.
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
