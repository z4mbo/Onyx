export default {
  providers: [
    {
      domain: process.env.CLERK_JWT_ISSUER_DOMAIN,
      applicationID: "convex",
    },
    {
      domain: process.env.CLERK_JWT_ISSUER_DOMAIN,
      applicationID: "5d25P5uFMQsN6W2D",
    },
  ],
}
