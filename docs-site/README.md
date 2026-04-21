# Gum Docs Site

This folder contains the public Mintlify documentation site for Gum.

The root `docs/` folder is for internal product and architecture specs. Do not publish it directly.

## Local Preview

Mintlify requires an LTS Node version. This docs site pins Node `20.17.0`.

```bash
npm i -g mint
cd docs-site
mint dev
```

Validate before pushing:

```bash
mint validate
mint broken-links
```

## Deploy

Connect Mintlify to this repository and set the docs source directory to `docs-site`.

The recommended production URL is a docs subdomain, for example:

```text
https://docs.yourdomain.com
```

Set the marketing site redirect to the same URL:

```bash
NEXT_PUBLIC_GUM_DOCS_URL="https://docs.yourdomain.com"
```
