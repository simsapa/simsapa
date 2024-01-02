const path = require('path');

module.exports = {
  entry: './src-ts/simsapa.ts',
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
    filename: 'simsapa.min.js',
    path: path.resolve(__dirname, 'simsapa/assets/js/'),
  },
};
