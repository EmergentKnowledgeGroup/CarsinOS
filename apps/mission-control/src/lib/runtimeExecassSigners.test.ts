import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { invoke } from "@tauri-apps/api/core";
import {
  fixtureDecisionProofBinding,
  fixtureIntakeRequest,
  fixtureMutationAuthorization,
  fixtureRunControlRequest,
} from "../glass/execass/fixtures";
import {
  signExecassLocalDecision,
  signExecassLocalOwnerIntake,
  signExecassLocalOwnerMutation,
  signExecassLocalRunControl,
} from "./runtime";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const PROOF = {
  authenticated_client_id: "carsinos-desktop",
  request_correlation_id: "corr-1",
  proof_hex: "ab".repeat(32),
};

beforeEach(() => {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  vi.mocked(invoke).mockResolvedValue(PROOF);
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  vi.clearAllMocks();
});

describe("signExecassLocalRunControl", () => {
  test("invokes the registered Tauri command with the exact binding", async () => {
    const { binding } = fixtureRunControlRequest("global_stop");
    const proof = await signExecassLocalRunControl(binding);
    expect(proof).toEqual(PROOF);
    expect(invoke).toHaveBeenCalledWith("sign_execass_local_run_control", {
      binding,
    });
  });

  test("refuses outside the desktop shell so proofs never come from the browser", async () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    const { binding } = fixtureRunControlRequest("global_stop");
    await expect(signExecassLocalRunControl(binding)).rejects.toThrow(
      /desktop/i,
    );
    expect(invoke).not.toHaveBeenCalled();
  });

  test("does not treat an arbitrary browser URL as a desktop signer", async () => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    window.history.replaceState({}, "", "/?unrelated=1");
    const { binding } = fixtureRunControlRequest("global_stop");
    await expect(signExecassLocalRunControl(binding)).rejects.toThrow(
      /desktop/i,
    );
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("signExecassLocalOwnerIntake", () => {
  test("passes the full intake request to the native signer", async () => {
    const request = fixtureIntakeRequest();
    await signExecassLocalOwnerIntake(request);
    expect(invoke).toHaveBeenCalledWith("sign_execass_local_owner_intake", {
      request,
    });
  });
});

describe("signExecassLocalOwnerMutation", () => {
  test("passes the mutation binding to the native signer", async () => {
    const { binding } = fixtureMutationAuthorization("policy_update");
    await signExecassLocalOwnerMutation(binding);
    expect(invoke).toHaveBeenCalledWith("sign_execass_local_owner_mutation", {
      binding,
    });
  });
});

describe("signExecassLocalDecision", () => {
  test("passes the binding and correlation id to the native signer", async () => {
    const binding = fixtureDecisionProofBinding();
    await signExecassLocalDecision(binding, "corr-decision-1");
    expect(invoke).toHaveBeenCalledWith("sign_execass_local_decision", {
      binding,
      requestCorrelationId: "corr-decision-1",
    });
  });
});
