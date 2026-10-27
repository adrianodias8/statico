# Portfolio

A freelancer's site as a small operating system on a white-phosphor CRT:
it boots with your logo, then opens on a desktop of volumes (services,
experience, projects, writing, profile, contact, and a working terminal)
that open as windows you can drag, stack and close. Booking a call is always
in the menu bar.

Without JavaScript, or with reduced motion, it stays fully usable: every
window is an ordinary section of one readable page, and the boot screen is
skipped. Visitors can switch the CRT effect and the (off by default) sound
in the menu bar.

```bash
statico dev       # preview at http://localhost:3000
statico check     # links, alt text, front matter
statico build https://your-domain.example
```

## Make it yours

| File | What to change |
|---|---|
| `statico.json` | name, description, email, booking link, GitHub |
| `includes/logo.html`, `includes/logo-mark.html` | your logo (inline SVG, drawn in `currentColor`) |
| `collections/services.json` | what you offer |
| `collections/experience.json` | where you've worked |
| `collections/projects.json` | something you built |
| `collections/volumes.json` | the desktop's volumes and their file names |
| `pages/index.html` | the boot log lines and the window contents |
| `content/writing/*.md` | posts (title, description, date) |
| `pages/about.html` | your bio |
| `assets/js/os.js` | the terminal's commands |

The palette is a handful of tokens at the top of `assets/css/main.css`.

This copy ships with Adriano Dias's real content and logo as the example.
Replace them.

Fonts: VT323 and Schibsted Grotesk, both SIL Open Font License (see `assets/fonts/`).
