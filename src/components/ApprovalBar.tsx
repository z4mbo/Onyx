import { ShieldAlert } from "lucide-solid"
import type { Component } from "solid-js"
import type { ApprovalRequest } from "../lib/types"

export const ApprovalBar: Component<{
  request: ApprovalRequest
  busy: boolean
  onRespond: (allow: boolean) => void
}> = (props) => (
  <section class="approval-bar" role="alertdialog" aria-label="Tool approval">
    <div class="approval-icon"><ShieldAlert size={18} /></div>
    <div class="approval-copy">
      <strong>{props.request.title}</strong>
      <code>{props.request.detail}</code>
      <span>This action {props.request.risk}.</span>
    </div>
    <div class="approval-actions">
      <button class="secondary-button" disabled={props.busy} onClick={() => props.onRespond(false)}>Deny</button>
      <button class="primary-button" disabled={props.busy} onClick={() => props.onRespond(true)}>Allow once</button>
    </div>
  </section>
)
