# Studios news

Articles shown in the app's News panel next to Modrinth's news, and opened in an in-app popup.

## Adding an article

1. Make a folder named after the article's slug, e.g. `studio-news/client-installations/`.
2. Put the article in `article.md` (plain Markdown, no front matter). Images and videos go in the
   same folder and are linked relatively: `![Alt text](./screenshot.png)`.
3. Add a banner image to the folder (16:9 or 2:1 works best, e.g. 1600×800).
4. Add an entry to `index.json`. Studios articles are sorted by date, newest first. The feature chronology is News, then Play mode, then Client installations, so the sidebar displays Client installations above Play mode above News. The sidebar shows the latest three Studios articles followed by two Modrinth articles.

Example entry:

```json
{
	"slug": "client-installations",
	"title": "Client installations",
	"summary": "One line shown on the news card.",
	"date": "2026-11-18T12:00:00+01:00",
	"version": "1.4.0",
	"banner": "banner.png",
	"popup": true
}
```

- `version`: the app version the article belongs to.
- `popup`: `true` opens the article once, automatically, the first time someone starts that
  version. Use it for big updates only.

## Where the app reads it

- **While developing** (`pnpm app:dev`): straight from this folder, so you can write and preview an
  article before pushing it. Edit, save, reopen the article.
- **In releases**: from the `studio` branch on GitHub
  (`raw.githubusercontent.com/Tigrolino/modrinth-studios/studio/studio-news/`), so a new article
  shows up as soon as it's pushed, without an app update. The copy of this folder that's built into
  the app is the fallback when GitHub can't be reached.

## Images

Article images are drawn, not screenshotted, in the style of Modrinth's own news images: the app
window on a coloured gradient, and a thumbnail with a block pattern and a glossy icon. No real
accounts, skins or files. `client-installations/make-images.py` draws that article's images (needs
`pip install cairosvg` and the Inter font); copy it as a starting point for a new article.

The News and Play mode banners share the Client installations banner's isometric block pattern and glass finish. Their editable `banner.svg` sources sit beside the rendered `banner.webp` files: warm amber/coral for News, blue/cyan/mint for Play mode, and the existing purple for Client installations.
