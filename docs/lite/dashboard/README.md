# ObservaLog Lite — Incident Forensics UI

React + TypeScript dashboard for the Lite Brain API. The dashboard is diagnosis-first: active finding, affected-service cluster, linked event records, reasoning/alternatives, and paginated diagnosis history.

## Local development

```sh
cd docs/lite/dashboard
npm install
npx shadcn@latest add @cult-ui/halo-button --overwrite --yes
npm run dev
```

Production build: `npm run build`. Browser tests: `npx playwright install chromium && npx playwright test`.

## Data and safety

- Preview records are labeled demo and are not live.
- Connect the Brain URL and token to fetch `/api/findings` and `/api/logs`.
- Service nodes represent affected-service metadata only. This API does not expose causal edges yet; the UI intentionally does not invent cause-to-symptom links.
- Read-only polling is every five seconds; SSE has not been implemented.

## Deployment

`.github/workflows/dashboard-build-deploy.yml` installs the actual Cult UI Halo Button from the shadcn registry, type-checks/builds, runs Playwright browser E2E, and deploys the built `dist` directory to GitHub Pages.
