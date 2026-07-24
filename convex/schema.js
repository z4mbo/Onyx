import { defineSchema, defineTable } from "convex/server"
import { v } from "convex/values"

export default defineSchema({
  snapshots: defineTable({
    owner: v.string(),
    payload: v.string(),
    updatedAt: v.number(),
  }).index("by_owner", ["owner"]),
})
