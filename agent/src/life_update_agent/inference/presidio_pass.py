"""Layer 4 - contextual PII redaction via Presidio.

Distinct from the Layer 2 regex/entropy pass: this catches PII that isn't
shape-based (names, addresses, other context-sensitive info) using spaCy
NER. Runs only inside the idle-triggered inference worker, never inline
during capture, since it's too slow for a real-time path.

The analyzer/anonymizer engines load a spaCy model on first use (~1s) and
are cached for the life of the process.
"""

from __future__ import annotations

from functools import lru_cache

from presidio_analyzer import AnalyzerEngine
from presidio_analyzer.nlp_engine import NlpEngineProvider
from presidio_anonymizer import AnonymizerEngine
from presidio_anonymizer.entities import OperatorConfig

CONTEXTUAL_ENTITIES = ["PERSON", "LOCATION", "NRP"]


@lru_cache(maxsize=1)
def _get_engines(spacy_model: str) -> tuple[AnalyzerEngine, AnonymizerEngine]:
    nlp_config = {
        "nlp_engine_name": "spacy",
        "models": [{"lang_code": "en", "model_name": spacy_model}],
    }
    nlp_engine = NlpEngineProvider(nlp_configuration=nlp_config).create_engine()
    analyzer = AnalyzerEngine(nlp_engine=nlp_engine, supported_languages=["en"])
    anonymizer = AnonymizerEngine()
    return analyzer, anonymizer


def redact_contextual_pii(text: str, spacy_model: str = "en_core_web_sm") -> str:
    if not text or not text.strip():
        return text

    analyzer, anonymizer = _get_engines(spacy_model)
    results = analyzer.analyze(text=text, language="en", entities=CONTEXTUAL_ENTITIES)
    if not results:
        return text

    anonymized = anonymizer.anonymize(
        text=text,
        analyzer_results=results,
        operators={"DEFAULT": OperatorConfig("replace", {"new_value": "[REDACTED]"})},
    )
    return anonymized.text
