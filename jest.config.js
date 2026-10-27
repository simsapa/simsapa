module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'jsdom',
  roots: ['<rootDir>/src-ts', '<rootDir>/npm'],
  testMatch: ['**/*.test.ts', '**/*.test.js'],
  collectCoverageFrom: [
    'src-ts/**/*.ts',
    '!src-ts/**/*.test.ts',
  ],
  setupFilesAfterEnv: ['<rootDir>/src-ts/test-setup.ts']
};