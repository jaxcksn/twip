import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import type { PrismTheme } from 'prism-react-renderer';

// Black and white code: emphasis by weight and tone, not hue.
const mono = (fg: string, bg: string, muted: string): PrismTheme => ({
  plain: { color: fg, backgroundColor: bg },
  styles: [
    { types: ['comment', 'prolog', 'doctype', 'cdata'], style: { color: muted, fontStyle: 'italic' } },
    { types: ['keyword', 'builtin', 'important', 'atrule'], style: { fontWeight: 'bold' } },
    { types: ['punctuation', 'operator'], style: { color: muted } },
  ],
});

const config: Config = {
  title: 'Twip',
  tagline: 'A DOCX layout, editing and rendering engine.',
  favicon: 'img/logo.svg',
  url: 'https://jaxcksn.github.io',
  baseUrl: '/twip/',
  organizationName: 'jaxcksn',
  projectName: 'twip',
  trailingSlash: false,
  onBrokenLinks: 'throw',
  i18n: { defaultLocale: 'en', locales: ['en'] },

  // Source Serif 4 from Google Fonts: every weight, italics, and the optical-size axis.
  headTags: [
    { tagName: 'link', attributes: { rel: 'preconnect', href: 'https://fonts.googleapis.com' } },
    { tagName: 'link', attributes: { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: 'anonymous' } },
  ],
  stylesheets: [
    'https://fonts.googleapis.com/css2?family=Source+Serif+4:ital,opsz,wght@0,8..60,200..900;1,8..60,200..900&display=swap',
  ],

  presets: [
    [
      'classic',
      {
        docs: {
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/jaxcksn/twip/edit/main/docs/',
        },
        blog: false,
        theme: { customCss: './src/css/custom.css' },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    colorMode: { respectPrefersColorScheme: true },
    navbar: {
      logo: { alt: 'Twip', src: 'img/logo.svg', srcDark: 'img/logo-dark.svg' },
      items: [{ href: 'https://github.com/jaxcksn/twip', label: 'GitHub', position: 'right' }],
    },
    footer: {
      copyright: `Copyright ${new Date().getFullYear()} Jackson Casey. Apache License 2.0.`,
    },
    prism: {
      theme: mono('#000000', '#f5f5f5', '#6b6b6b'),
      darkTheme: mono('#ffffff', '#141414', '#9a9a9a'),
      additionalLanguages: ['rust', 'csharp', 'toml', 'bash'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
