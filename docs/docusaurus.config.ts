import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import type { PrismTheme } from 'prism-react-renderer';
import include from './src/remark/include';

// Ink code: emphasis by weight and tone, with strings in the accent.
const mono = (fg: string, bg: string, muted: string, accent: string): PrismTheme => ({
  plain: { color: fg, backgroundColor: bg },
  styles: [
    { types: ['comment', 'prolog', 'doctype', 'cdata'], style: { color: muted, fontStyle: 'italic' } },
    { types: ['keyword', 'builtin', 'important', 'atrule'], style: { fontWeight: '600' } },
    { types: ['punctuation', 'operator'], style: { color: muted } },
    { types: ['string', 'char', 'attr-value', 'regex'], style: { color: accent } },
  ],
});

const config: Config = {
  title: 'Twip',
  tagline: 'A DOCX layout, editing and rendering engine.',
  favicon: 'img/favicon.ico',
  url: 'https://jaxcksn.github.io',
  baseUrl: '/twip/',
  organizationName: 'jaxcksn',
  projectName: 'twip',
  trailingSlash: false,
  onBrokenLinks: 'throw',
  i18n: { defaultLocale: 'en', locales: ['en'] },

  // Source Serif 4 (editorial), DM Sans (interface) and IBM Plex Mono (technical) from Google Fonts.
  headTags: [
    { tagName: 'link', attributes: { rel: 'icon', type: 'image/svg+xml', href: '/twip/img/favicon.svg' } },
    { tagName: 'link', attributes: { rel: 'apple-touch-icon', href: '/twip/img/apple-touch-icon.png' } },
    { tagName: 'link', attributes: { rel: 'preconnect', href: 'https://fonts.googleapis.com' } },
    { tagName: 'link', attributes: { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: 'anonymous' } },
  ],
  stylesheets: [
    'https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,400..700;1,9..40,400..700&family=IBM+Plex+Mono:wght@400;500;600&family=Source+Serif+4:ital,opsz,wght@0,8..60,200..900;1,8..60,200..900&display=swap',
  ],

  presets: [
    [
      'classic',
      {
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/jaxcksn/twip/edit/main/docs/',
          remarkPlugins: [include],
        },
        blog: false,
        theme: { customCss: './src/css/custom.css' },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/social-card.png',
    colorMode: { respectPrefersColorScheme: true },
    navbar: {
      logo: { alt: 'Twip', src: 'img/logo.svg', srcDark: 'img/logo-dark.svg' },
      items: [
        { type: 'docSidebar', sidebarId: 'docs', label: 'Docs', position: 'left' },
        // The generated references, built beside the site (.github/workflows/docs.yml).
        { to: 'pathname:///api/web/', label: 'TypeScript API', position: 'left' },
        { to: 'pathname:///api/dotnet/', label: '.NET API', position: 'left' },
        { href: 'https://github.com/jaxcksn/twip', label: 'GitHub', position: 'right' },
      ],
    },
    footer: {
      copyright: `Layout. Edit. Render. · Copyright ${new Date().getFullYear()} Jackson Casey. Apache License 2.0.`,
    },
    prism: {
      theme: mono('#171817', '#ffffff', '#646660', '#0968cd'),
      darkTheme: mono('#f8f7f3', '#1d1e1c', '#a7a9a2', '#61a6ff'),
      additionalLanguages: ['rust', 'csharp', 'toml', 'bash'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
