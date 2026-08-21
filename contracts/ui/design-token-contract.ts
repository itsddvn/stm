export const designTokens = {
  colorStrategy: "restrained",
  accentMaximumPercent: 10,
  spacingScale: [4, 8, 12, 16, 24, 32, 48, 64, 96],
  radiusScale: [0, 4, 8],
  motionMs: [100, 150, 200, 250],
  fontBody: "Source Sans 3",
  fontDisplay: "Barlow Condensed",
  fontPackages: {
    "@fontsource/barlow-condensed": "5.3.0",
    "@fontsource/source-sans-3": "5.3.0",
  },
  depthStrategy: "hairline-borders-and-surface-tints",
  minimumViewport: { width: 1024, height: 720 },
  reviewViewports: [
    { width: 1024, height: 720 },
    { width: 1280, height: 800 },
    { width: 1440, height: 900 },
  ],
} as const;
