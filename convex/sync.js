import { v } from "convex/values"
import { mutation, query } from "./_generated/server"

export const upsertSnapshot = mutation({
  args: { payload: v.string() },
  handler: async (ctx, { payload }) => {
    const identity = await ctx.auth.getUserIdentity()
    if (!identity) throw new Error("Authentication required")
    if (payload.length > 4_000_000) throw new Error("Snapshot exceeds the 4 MB limit")
    const existing = await ctx.db.query("snapshots").withIndex("by_owner", (q) => q.eq("owner", identity.subject)).unique()
    const value = { owner: identity.subject, payload, updatedAt: Date.now() }
    if (existing) await ctx.db.patch(existing._id, value)
    else await ctx.db.insert("snapshots", value)
  },
})

export const latestSnapshot = query({
  args: {},
  handler: async (ctx) => {
    const identity = await ctx.auth.getUserIdentity()
    if (!identity) throw new Error("Authentication required")
    const snapshot = await ctx.db.query("snapshots").withIndex("by_owner", (q) => q.eq("owner", identity.subject)).unique()
    return snapshot?.payload ?? null
  },
})
