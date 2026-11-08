//! Original, authored learning material. Outcomes come from the worker, not this text.

pub struct Step {
    pub title: &'static str,
    pub objective: &'static str,
    pub explanation: &'static str,
    pub tasks: &'static [&'static str],
    pub hint: &'static str,
    pub solution: &'static str,
}

pub const STEPS: [Step; 7] = [
    Step {
        title: "One approval",
        objective: "Predict whether BuildA is ready with only its automated-check approval.",
        explanation: "A release needs two approvals for the same build. TestRunner has approved BuildA. Read the policy and the evidence before you run the claim.",
        tasks: &[
            "Read the policy and the accepted evidence.",
            "Choose a prediction, then run the claim.",
            "Find the condition that has no support.",
        ],
        hint: "The policy joins two conditions with AND. Which approval is present? Which one is missing?",
        solution: "TestRunner's approval supports one condition. Reviewer's approval of BuildA has not been asserted or derived. Under this closed-world profile, readiness is not derivable: FALSE. That does not prove the real build unsafe.",
    },
    Step {
        title: "A different build",
        objective: "Add Reviewer's approval of BuildB, then predict again.",
        explanation: "There will be two approvals, but they concern different builds. The variable $build denotes the same entity everywhere it appears in this policy.",
        tasks: &[
            "Add approves(Reviewer, BuildB).",
            "Predict and run BuildA's readiness claim.",
            "Compare the build named by each approval.",
        ],
        hint: "An approval of BuildB cannot fill a condition that requires an approval of BuildA.",
        solution: "With $build bound to BuildA, the second condition is approves(Reviewer, BuildA). The new evidence is about BuildB. BuildA's readiness remains not derivable.",
    },
    Step {
        title: "Same build, both approvals",
        objective: "Add Reviewer's approval of BuildA and inspect what supports the result.",
        explanation: "Give BuildA its review. After running, follow the conclusion to the policy and the assertions used to establish its two conditions.",
        tasks: &[
            "Add approves(Reviewer, BuildA).",
            "Predict, then run the claim.",
            "Inspect the supporting assertions and their source labels.",
        ],
        hint: "Both conditions now concern BuildA. Inspect the proof to find the actual assertion identifiers.",
        solution: "The checker and reviewer approvals now bind the same build variable. The engine can apply the AND policy and derive readiness from those inputs. A proof establishes that derivation; it does not independently verify the source documents.",
    },
    Step {
        title: "Replay and proof",
        objective: "Distinguish the operations the engine attempted from the derivation it returned.",
        explanation: "Execution replay includes attempts, intermediate states and failed branches. A proof explains the returned derivation. They answer different questions about the same run.",
        tasks: &[
            "Run the current claim if no run is available.",
            "Inspect execution replay and a failed attempt, when present.",
            "Open the proof, then answer the understanding check.",
        ],
        hint: "A rejected candidate can appear in execution history without supporting the final derivation.",
        solution: "Replay reads the saved operations and snapshots. It never runs reasoning again. The proof links the steps and assertions relevant to its derivation; it is not a transcript of every operation attempted.",
    },
    Step {
        title: "Independent support",
        objective: "Add a second review assertion, withdraw one, then withdraw the last.",
        explanation: "Two separately recorded assertions can support the same claim. Removing one assertion need not remove the conclusion if another active assertion still supports it.",
        tasks: &[
            "Assert BuildA's review again with a different source label.",
            "Predict and run while both review assertions are active.",
            "Withdraw one review, run, then withdraw the last and run.",
        ],
        hint: "Count active assertions, not just distinct statement text. Each withdrawal names one assertion identifier.",
        solution: "Readiness survives the first withdrawal because one review assertion remains. After the last review support is withdrawn, the AND policy lacks its second condition. The engine rebuilds from the surviving assertions. Separate records do not by themselves establish independent real-world evidence.",
    },
    Step {
        title: "Assert readiness directly",
        objective: "Try to assert the protected conclusion and explain the refusal.",
        explanation: "The readiness relation is declared derived-only. Evidence is admitted through the policy's premises; typing the conclusion directly cannot bypass that policy.",
        tasks: &[
            "Attempt to add ready(BuildA, Release).",
            "Inspect the admission refusal and unchanged revision.",
            "Choose the explanation that describes the boundary.",
        ],
        hint: "A refusal describes an attempted knowledge change. It is separate from the outcome of a reasoning query.",
        solution: "The engine refuses a direct readiness assertion because that relation is derived-only. No change is committed. Supply the policy's required evidence instead. Understanding this refusal is a successful learning check.",
    },
    Step {
        title: "Repair a flawed policy",
        objective: "Repair an OR policy so both approvals are required for the same build.",
        explanation: "Work in a separate exercise. The starting rule admits either approval. A single successful example cannot reveal that bug, so test four cases before and after your edit.",
        tasks: &[
            "Run all four cases against the flawed policy.",
            "Change OR (|) to AND (&), then rerun.",
            "Explain why the missing-approval and cross-build cases matter.",
        ],
        hint: "The checker-only case passes the weak rule. Does 'either condition' express the requirement that both approvals exist?",
        solution: "AND requires both approvals with the same $build binding. The checker-only and reviewer-only cases become FALSE, same-build approvals yield TRUE, and cross-build approvals remain FALSE. The case checks test behavior; compare your written explanation with these criteria yourself.",
    },
];

pub const FOUNDATIONS: [(&str, &str); 6] = [
    (
        "Prediction versus derivation",
        "Distinguish what you expect from what follows from stated inputs.",
    ),
    (
        "Precise claims",
        "Express argument roles, shared variables and scope without losing intent.",
    ),
    (
        "Evidence",
        "Trace a conclusion to its assertions and review what those inputs establish.",
    ),
    (
        "Reasoning outcomes",
        "Read four outcomes precisely and separate them from errors and refusals.",
    ),
    (
        "Investigating and revising a decision",
        "Predict, run, inspect, change evidence and repair a release policy.",
    ),
    (
        "Comparing neuro-symbolic approaches",
        "Compare architectures, their failure modes and maintenance costs.",
    ),
];

pub const REFERENCES: [(&str, &str, &str); 8] = [
    (
        "derivable",
        "Derivable",
        "A claim is derivable when the engine can establish it from the accepted statements and rules under the selected profile. Inspect its proof to see which inputs support it.",
    ),
    (
        "closed-world",
        "Closed world",
        "In this lesson, a claim with no derivation is FALSE. This says what follows from the present inputs; it is not a proof that the opposite is true in the world.",
    ),
    (
        "assertion",
        "Statement and assertion",
        "A statement is formal text. An assertion is a particular accepted record of that text with its own identifier and source label. The same statement may have several active assertion records.",
    ),
    (
        "variable",
        "Shared variables",
        "The same variable name inside this rule denotes the same entity. Both approval conditions must use the same build; two approvals for different builds cannot satisfy it.",
    ),
    (
        "replay",
        "Execution and proof",
        "Execution replay shows operations actually attempted in a saved run. The proof explains the result's derivation. A failed attempt can appear in replay without a corresponding successful proof step.",
    ),
    (
        "refusal",
        "Admission refusal",
        "An attempted knowledge change may be refused. Read the reported reason, repair the input or supply the required evidence, and try again. Refusal is separate from TRUE, FALSE, UNKNOWN and RESOURCE_EXCEEDED.",
    ),
    (
        "depth",
        "A depth limit was reached",
        "The run did not finish within its configured search depth. Inspect the recorded boundary, increase depth or simplify the model, and rerun. A browser depth limit is separate from host fuel and memory limits.",
    ),
    (
        "sources",
        "Source labels and trust",
        "Source labels let you identify and withdraw particular assertions. They do not authenticate a document or establish its accuracy. Review the underlying evidence and its intended formalization.",
    ),
];
