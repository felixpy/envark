export default {
  extends: ['@commitlint/config-conventional'],
  rules: {
    // Dependabot generates long version tables; keep all other message rules strict.
    'body-max-line-length': [process.env.DEPENDABOT_PR === 'true' ? 1 : 2, 'always', 100],
  },
}
