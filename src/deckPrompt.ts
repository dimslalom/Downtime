/** Handed to another AI to build a deck. It asks for content and evidence only. */
export const DECK_PROMPT = `You are building a flashcard deck from the course material I provide. Each card is studied during gaming downtime, so reading, recalling, revealing and grading should take about 15 seconds.

First, map the course capabilities: what students must explain, distinguish, calculate, derive, interpret or construct. Then select cards that support those capabilities.

Select:
- Useful definitions, notation, relationships, conditions, exceptions and short method-choice scenarios.
- Conceptual prerequisites for complex tasks. Exclude extended calculations, full procedures and long proofs.
- Short application questions are welcome.

Exclude administrative details, incidental numbers, duplicates, and anything without a useful retrieval target.

Write each card so that it:
- Tests one focused retrieval or judgment.
- Makes sense on its own, in any order.
- Does not reveal the answer in its wording.
- Prefers a front under 20 words and a back under 15, but lets accuracy override brevity.
- Keeps any assumptions and qualifications needed to stay correct.

For each card, also provide:
- source: a genuine location in the material, such as "Lecture 5, slide 18". Never invent a reference. Use null if none exists.
- importance: an integer from 1 to 5, assigned from evidence in the course, such as emphasis, prerequisites or stated outcomes. Do not rank definitions above exceptions by default. Importance is a study priority. It does not claim a topic will appear on an exam.
- importance_reason: one line explaining that evidence.
- assessment_evidence: any explicit link to assessment in the material, otherwise null.
- card_type: exactly one of Definition, Relationship, Exception, Method Choice.
- id: a short, stable, unique id such as "shortest-paths-001". When revising existing cards, keep the supplied id for any card that still tests the same knowledge.

Omit unsupported claims and unresolved conflicts between sources. If the material has no suitable targets, return zero cards.

Do not invent memory states, personal difficulty, review outcomes or scheduling intervals. Those come from real reviews.

Return only valid JSON in exactly this format, with no commentary or code fence:

{
  "deck_name": "Algorithms",
  "cards": [
    {
      "id": "shortest-paths-001",
      "card_type": "Exception",
      "front": "Why can negative edge weights invalidate Dijkstra's greedy finalization?",
      "back": "A later path can reduce a distance already treated as final.",
      "source": "Lecture 5, slide 18",
      "importance": 4,
      "importance_reason": "Supports selecting and justifying shortest-path algorithms.",
      "assessment_evidence": null
    }
  ]
}`;
