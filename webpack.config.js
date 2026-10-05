const path = require('path');

module.exports = {
  // The timeline page has its own bundle, so that d3 is not loaded by every
  // sutta page.
  entry: {
    simsapa: './src-ts/simsapa.ts',
    timeline: './src-ts/timeline.ts',
  },
  mode: 'production',
  optimization: {
    minimize: true
  },
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: 'ts-loader',
        exclude: /node_modules/,
      },
    ],
  },
  resolve: {
    extensions: ['.tsx', '.ts', '.js'],
  },
  output: {
    filename: '[name].min.js',
    path: path.resolve(__dirname, 'assets/js/'),
  },
};
