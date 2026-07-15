<!--
  HostKeyDialog.svelte — Host-key trust prompt (TOFU).

  Shows whenever the prompts store holds a pending host-key prompt. For an
  UNKNOWN host it offers a straightforward trust decision. For a CHANGED key it
  renders an alarming MITM warning and gates the Accept button behind an
  explicit "I understand" checkbox — a changed key is never one click away from
  acceptance (tasks.md "Conventions & invariants"). Accepting persists the key
  to known_hosts on the backend.
-->
<script lang="ts">
  import { respondPrompt } from "$lib/ipc/commands";
  import { prompts } from "$lib/stores/prompts.svelte";
  import Modal from "$lib/components/common/Modal.svelte";

  let prompt = $derived(prompts.hostKey);
  let changed = $derived(prompt?.status === "CHANGED");
  let acknowledged = $state(false);

  /** Respond to the pending prompt and clear it. */
  async function respond(accept: boolean): Promise<void> {
    if (!prompt) return;
    const id = prompt.promptId;
    prompts.clearHostKey();
    acknowledged = false;
    await respondPrompt(id, { type: "hostKey", accept });
  }
</script>

{#if prompt}
  <Modal title={changed ? "⚠ Host key CHANGED" : "Unknown host key"} onClose={() => respond(false)}>
    <div class="content" class:changed>
      {#if changed}
        <p class="warn" role="alert">
          The host key for <strong>{prompt.host}</strong> does not match the key on record. This could
          mean a man-in-the-middle attack — or that the server was legitimately reinstalled. Do not continue
          unless you are certain.
        </p>
        {#if prompt.existingFingerprint}
          <p class="fp">Previously trusted: <code>{prompt.existingFingerprint}</code></p>
        {/if}
      {:else}
        <p>
          The authenticity of host <strong>{prompt.host}:{prompt.port}</strong> can't be established.
          Trust this key and continue connecting?
        </p>
      {/if}

      <dl class="details">
        <dt>Key type</dt>
        <dd>{prompt.keyType}</dd>
        <dt>Fingerprint</dt>
        <dd><code>{prompt.fingerprintSha256}</code></dd>
      </dl>

      {#if changed}
        <label class="ack">
          <input type="checkbox" bind:checked={acknowledged} />
          I understand the risk and want to replace the stored key.
        </label>
      {/if}

      <div class="actions">
        <button type="button" onclick={() => respond(false)}>Reject</button>
        <button
          type="button"
          class:danger={changed}
          class:primary={!changed}
          disabled={changed && !acknowledged}
          onclick={() => respond(true)}
        >
          {changed ? "Replace & connect" : "Trust & connect"}
        </button>
      </div>
    </div>
  </Modal>
{/if}

<style>
  .content {
    display: flex;
    flex-direction: column;
    gap: 12px;
    font-size: 0.88rem;
  }
  .warn {
    background: #fdecea;
    color: #8a1c12;
    border: 1px solid #f5b7b1;
    border-radius: 6px;
    padding: 10px;
    margin: 0;
  }
  .fp {
    margin: 0;
    font-size: 0.8rem;
  }
  .details {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 0;
  }
  .details dt {
    color: var(--muted, #666);
  }
  .details dd {
    margin: 0;
  }
  code {
    font-family: ui-monospace, monospace;
    word-break: break-all;
  }
  .ack {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    font-size: 0.85rem;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  button {
    padding: 6px 14px;
    border-radius: 6px;
    border: 1px solid var(--border, #c4c4c4);
    background: var(--surface, #fff);
    cursor: pointer;
  }
  button.primary {
    background: var(--accent, #396cd8);
    border-color: var(--accent, #396cd8);
    color: #fff;
  }
  button.danger {
    background: #c0392b;
    border-color: #c0392b;
    color: #fff;
  }
  button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
