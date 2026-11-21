#!/usr/bin/env python3
"""Drive bin/tohru-voice's wake gating, turn delivery and speech shaping without a microphone or models."""
import importlib.machinery
import importlib.util
from pathlib import Path
import unittest

try:
    import numpy
except ImportError:
    numpy = None

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader("tohru_voice", str(ROOT / "bin/tohru-voice"))
spec = importlib.util.spec_from_loader("tohru_voice", loader)
voice_module = importlib.util.module_from_spec(spec)
loader.exec_module(voice_module)


class FakeRecognizer:
    def __init__(self, partials, final):
        self.partials = list(partials)
        self.final = final
        self.fed = 0
        self.dropped = False

    def start(self):
        self.dropped = False

    def feed(self, window):
        self.fed += 1

    def partial(self):
        return self.partials.pop(0) if len(self.partials) > 1 else self.partials[0]

    def finish(self):
        return "" if self.dropped else self.final

    def drop(self):
        self.dropped = True


class FakeSpotter:
    def __init__(self, hit_at=None):
        self.hit_at = hit_at
        self.fed = 0
        self.flushed = 0
        self.stream = None

    def start(self):
        self.stream = object()

    def feed(self, samples):
        self.fed += 1
        return self.hit_at is not None and self.fed >= self.hit_at

    def flush(self):
        self.flushed += 1
        return False

    def drop(self):
        self.stream = None


class FakeEngine:
    def __init__(self):
        self.loads = 0
        self.unloads = 0
        self.alive = False

    def load(self):
        self.loads += 1
        self.alive = True

    def unload(self):
        self.unloads += 1
        self.alive = False


class FakeSpeaker:
    speaking = False
    level = 0.0
    echo = 0.0
    quiet = False

    def __init__(self):
        self.said = []
        self.engine = FakeEngine()
        self.hushed = 0

    def echo_level(self):
        return self.echo

    def deaf(self):
        return self.quiet

    def say(self, text, conversation):
        self.said.append((text, conversation))

    def hush(self):
        self.hushed += 1


def make_voice(recognizer, spotter=None):
    voice = object.__new__(voice_module.Voice)
    voice.spotter = spotter
    voice.np = None
    voice.recognizer = recognizer
    voice.turn = None
    voice.speaker = FakeSpeaker()
    voice.paused_reason = None
    voice.conversation_until = 0.0
    voice.thinking = False
    voice.load_on_conversation = False
    voice.reset_utterance()
    return voice


class VoiceTests(unittest.TestCase):
    def setUp(self):
        self.events = []
        self.real_emit = voice_module.emit
        voice_module.emit = lambda event, **fields: self.events.append(dict(fields, event=event))

    def tearDown(self):
        voice_module.emit = self.real_emit

    def speak(self, voice, windows, quiet):
        voice.begin([])
        for _ in range(windows):
            voice.hear(b"", True)
        for _ in range(quiet):
            if not voice.in_speech:
                break
            voice.hear(b"", False)

    def test_wake_word_forms(self):
        self.assertEqual(voice_module.wake_split("TOHRU WHAT TIME IS IT"), "What time is it")
        self.assertEqual(voice_module.wake_split("toru"), "")
        self.assertEqual(voice_module.wake_split("TO RU tell me a joke"), "Tell me a joke")
        self.assertEqual(voice_module.wake_split("TERU WHAT'S THE WEATHER"), "What's the weather")
        self.assertIsNone(voice_module.wake_split("WHAT TIME IS IT TOHRU"))
        self.assertIsNone(voice_module.wake_split("TOTALLY"))
        self.assertTrue(voice_module.wake_possible("TO"))
        self.assertFalse(voice_module.wake_possible("WHAT"))

    def test_speech_without_the_wake_word_is_discarded_unheard(self):
        recognizer = FakeRecognizer(["WHAT", "WHAT TIME IS IT"], "WHAT TIME IS IT")
        voice = make_voice(recognizer)
        self.speak(voice, 20, 40)
        self.assertTrue(recognizer.dropped)
        self.assertFalse(voice.in_speech)
        self.assertEqual(self.events, [])

    def test_wake_word_opens_a_conversation_and_sends_the_request(self):
        recognizer = FakeRecognizer(["TO", "TOHRU WHAT"], "TOHRU WHAT TIME IS IT")
        voice = make_voice(recognizer)
        self.speak(voice, 20, 40)
        self.assertEqual([e["event"] for e in self.events], ["conversation", "turn"])
        self.assertEqual(self.events[1]["text"], "What time is it")
        self.assertTrue(voice.in_conversation() and voice.thinking)

    def test_follow_ups_in_a_conversation_need_no_wake_word(self):
        voice = make_voice(FakeRecognizer(["AND TOMORROW"], "AND TOMORROW"))
        voice.extend_conversation()
        self.speak(voice, 20, 40)
        self.assertEqual(self.events, [{"text": "And tomorrow", "event": "turn"}])

    def test_the_keyword_spotter_wakes_the_recognizer_with_the_buffered_speech(self):
        recognizer = FakeRecognizer(["TOHRU WHAT"], "TOHRU WHAT TIME IS IT")
        voice = make_voice(recognizer, FakeSpotter(hit_at=12))
        self.speak(voice, 20, 40)
        self.assertGreaterEqual(recognizer.fed, 12)
        self.assertEqual([e["event"] for e in self.events], ["conversation", "turn"])
        self.assertEqual(self.events[1]["text"], "What time is it")

    def test_speech_the_spotter_does_not_flag_never_reaches_the_recognizer(self):
        recognizer = FakeRecognizer(["TOHRU"], "TOHRU WHAT TIME IS IT")
        spotter = FakeSpotter()
        voice = make_voice(recognizer, spotter)
        self.speak(voice, 60, 40)
        self.assertEqual(recognizer.fed, 0)
        self.assertEqual(spotter.flushed, 1)
        self.assertEqual(self.events, [])

    def test_the_transcript_still_confirms_what_the_spotter_heard(self):
        recognizer = FakeRecognizer(["TOTALLY"], "TOTALLY FINE")
        voice = make_voice(recognizer, FakeSpotter(hit_at=3))
        self.speak(voice, 20, 40)
        self.assertEqual(self.events, [])

    def test_follow_ups_skip_the_spotter(self):
        recognizer = FakeRecognizer(["AND TOMORROW"], "AND TOMORROW")
        spotter = FakeSpotter()
        voice = make_voice(recognizer, spotter)
        voice.extend_conversation()
        self.speak(voice, 20, 40)
        self.assertEqual(spotter.fed, 0)
        self.assertEqual(self.events, [{"text": "And tomorrow", "event": "turn"}])

    def test_wake_word_alone_waits_for_the_request(self):
        voice = make_voice(FakeRecognizer(["TOHRU"], "TOHRU"))
        self.speak(voice, 12, 40)
        self.assertEqual([e["event"] for e in self.events], ["conversation", "wake"])

    def test_answers_are_spoken_plainly_and_not_while_paused(self):
        voice = make_voice(FakeRecognizer([""], ""))
        voice.command({"cmd": "say", "text": "**Eight** ounces. See [the chart](https://example.com).\n```\ncode\n```", "conversation": "c1"})
        self.assertEqual(voice.speaker.said, [("Eight ounces. See the chart. (code in the popup)", "c1")])
        voice.paused_reason = "locked"
        voice.command({"cmd": "say", "text": "Hello", "conversation": "c1"})
        self.assertEqual(len(voice.speaker.said), 1)
        long = voice_module.speakable("A sentence here. " * 80)
        self.assertLessEqual(len(long), voice_module.SPOKEN_CHARS + 30)
        self.assertTrue(long.endswith("The rest is in the chat."))

    def test_tts_stays_loaded_by_default_or_follows_the_conversation(self):
        import threading
        started = []
        real = threading.Thread
        voice_module.threading.Thread = lambda target, args=(), daemon=None: type("T", (), {"start": lambda self: (started.append(target), target(*args))})()
        try:
            voice = make_voice(FakeRecognizer(["TOHRU"], "TOHRU"))
            voice.conversation(True)
            voice.conversation(False)
            self.assertEqual((voice.speaker.engine.loads, voice.speaker.engine.unloads), (0, 0))
            voice.load_on_conversation = True
            voice.conversation(True)
            self.assertTrue(voice.speaker.engine.alive)
            voice.conversation(False)
            self.assertFalse(voice.speaker.engine.alive)
        finally:
            voice_module.threading.Thread = real

    @unittest.skipIf(numpy is None, "needs numpy")
    def test_the_dragon_does_not_answer_its_own_echo(self):
        voice = make_voice(FakeRecognizer(["TOHRU"], "TOHRU"))
        voice.np = numpy
        voice.silero = lambda window: 0.9
        voice.speaker.speaking = True
        voice.speaker.echo = 0.2
        echo = numpy.full(512, 0.25, dtype=numpy.float32)
        for _ in range(40):
            voice.step(echo)
        self.assertEqual(voice.speaker.hushed, 0)
        louder = numpy.full(512, 0.5, dtype=numpy.float32)
        for _ in range(10):
            voice.step(louder)
        self.assertEqual(voice.speaker.hushed, 1)
        self.assertEqual([e["event"] for e in self.events], ["barge"])
        voice.speaker.speaking = False
        voice.speaker.quiet = True
        voice.reset_utterance()
        for _ in range(10):
            voice.step(louder)
        self.assertFalse(voice.in_speech)


if __name__ == "__main__":
    unittest.main()
