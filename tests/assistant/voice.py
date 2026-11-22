#!/usr/bin/env python3
"""Drive bin/tohru-voice's wake gating, turn delivery and speech shaping without a microphone or models."""
import importlib.machinery
import importlib.util
from pathlib import Path
import tempfile
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
    paused = False
    level = 0.0
    echo = 0.0
    quiet = False

    def __init__(self):
        self.said = []
        self.engine = FakeEngine()
        self.hushed = 0
        self.pauses = 0
        self.resumes = 0
        self.spoken_text = ""

    def echo_level(self):
        return self.echo

    def deaf(self):
        return self.quiet

    def spoken_so_far(self):
        return self.spoken_text

    def say(self, text, conversation):
        self.said.append((text, conversation))

    def pause(self):
        self.paused = True
        self.pauses += 1

    def resume(self):
        self.paused = False
        self.resumes += 1

    def hush(self):
        self.paused = False
        self.speaking = False
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
    voice.cancelled_thinking = False
    voice.last_turn_text = ""
    voice.pending_continuation = ""
    voice.echo_cancelled = False
    voice.echo_coupling = voice_module.ECHO_COUPLING_INIT
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
        self.assertEqual(self.events, [{"on": True, "event": "hearing"}, {"on": False, "event": "hearing"},
                                       {"text": "And tomorrow", "event": "turn"}])

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
        self.assertEqual([e["event"] for e in self.events], ["hearing", "hearing", "turn"])

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
        voice = make_voice(FakeRecognizer(["WAIT STOP"], "WAIT STOP"))
        voice.np = numpy
        voice.silero = lambda window: 0.9
        voice.speaker.speaking = True
        voice.speaker.echo = 0.2
        echo = numpy.full(512, 0.25, dtype=numpy.float32)
        for _ in range(40):
            voice.step(echo)
        self.assertEqual((voice.speaker.pauses, voice.speaker.hushed), (0, 0))
        louder = numpy.full(512, 0.5, dtype=numpy.float32)
        for _ in range(10):
            voice.step(louder)
        self.assertEqual((voice.speaker.pauses, voice.speaker.hushed), (1, 1))
        self.assertEqual([e["event"] for e in self.events], ["hearing", "barge"])
        voice.speaker.speaking = False
        voice.speaker.quiet = True
        voice.reset_utterance()
        for _ in range(10):
            voice.step(louder)
        self.assertFalse(voice.in_speech)

    @unittest.skipIf(numpy is None, "needs numpy")
    def test_aec_uses_a_flat_barge_threshold(self):
        voice = make_voice(FakeRecognizer(["WAIT"], "WAIT"))
        voice.np = numpy
        voice.silero = lambda window: 0.9
        voice.echo_cancelled = True
        voice.speaker.speaking = True
        voice.speaker.echo = 0.35
        quiet_mic = numpy.full(512, 0.05, dtype=numpy.float32)
        for _ in range(20):
            voice.step(quiet_mic)
        self.assertEqual(voice.speaker.pauses, 0)
        normal_voice = numpy.full(512, 0.08, dtype=numpy.float32)
        for _ in range(7):
            voice.step(normal_voice)
        self.assertEqual(voice.speaker.pauses, 1)
        self.assertTrue(voice.barge_pending)

    @unittest.skipIf(numpy is None, "needs numpy")
    def test_echo_coupling_adapts_without_aec(self):
        voice = make_voice(FakeRecognizer(["WAIT"], "WAIT"))
        voice.np = numpy
        voice.silero = lambda window: 0.9
        voice.echo_cancelled = False
        voice.speaker.speaking = True
        voice.speaker.echo = 0.25
        # Room echo is ~0.16 of playback RMS; coupling should adapt down from 1.0 so normal speech can barge.
        room_echo = numpy.full(512, 0.04, dtype=numpy.float32)
        for _ in range(40):
            voice.step(room_echo)
        self.assertEqual(voice.speaker.pauses, 0)
        self.assertLess(voice.echo_coupling, 0.25)
        self.assertGreaterEqual(voice.echo_coupling, voice_module.ECHO_COUPLING_MIN)
        # 0.12 RMS is far below old 1.6 * 0.25 = 0.40, but well above 1.6 * coupling * 0.25.
        user_voice = numpy.full(512, 0.12, dtype=numpy.float32)
        for _ in range(7):
            voice.step(user_voice)
        self.assertEqual(voice.speaker.pauses, 1)

    @unittest.skipIf(numpy is None, "needs numpy")
    def test_pause_then_resume_on_noise_and_backchannels(self):
        for partial, final in (("", ""), ("MM", "MM"), ("MHM", "MHM"), ("UH HUH", "UH HUH"), ("YEAH", "YEAH"), ("RIGHT", "RIGHT"), ("OKAY", "OKAY")):
            self.events.clear()
            recognizer = FakeRecognizer([partial], final)
            voice = make_voice(recognizer)
            voice.np = numpy
            voice.echo_cancelled = True
            voice.speaker.speaking = True
            loud = numpy.full(512, 0.1, dtype=numpy.float32)
            # Fill ~1 s of pre-roll before crossing the barge window.
            voice.silero = lambda window: 0.0
            for _ in range(30):
                voice.step(loud)
            voice.silero = lambda window: 0.9
            for _ in range(7):
                voice.step(loud)
            self.assertEqual(voice.speaker.pauses, 1)
            self.assertEqual(voice.speaker.hushed, 0)
            self.assertGreaterEqual(recognizer.fed, 30)
            # Continue without real words until the ~1 s decision window expires.
            for _ in range(35):
                if not voice.barge_pending:
                    break
                voice.step(loud)
            self.assertFalse(voice.barge_pending)
            self.assertEqual(voice.speaker.resumes, 1)
            self.assertEqual(voice.speaker.hushed, 0)
            self.assertNotIn("barge", [e["event"] for e in self.events])
            self.assertNotIn("turn", [e["event"] for e in self.events])

    @unittest.skipIf(numpy is None, "needs numpy")
    def test_pause_then_barge_on_real_words(self):
        recognizer = FakeRecognizer(["", "WAIT WHAT", "WAIT WHAT ABOUT TOMORROW"], "WAIT WHAT ABOUT TOMORROW")
        voice = make_voice(recognizer)
        voice.np = numpy
        voice.echo_cancelled = True
        voice.speaker.speaking = True
        loud = numpy.full(512, 0.1, dtype=numpy.float32)
        voice.silero = lambda window: 0.0
        for _ in range(30):
            voice.step(loud)
        voice.silero = lambda window: 0.9
        for _ in range(7):
            voice.step(loud)
        self.assertEqual((voice.speaker.pauses, voice.speaker.hushed), (1, 0))
        self.assertGreaterEqual(recognizer.fed, 30)
        # First window partial is empty (stays paused); second has real words -> confirms barge.
        voice.step(loud)
        self.assertEqual(voice.speaker.hushed, 0)
        voice.step(loud)
        self.assertEqual(voice.speaker.hushed, 1)
        self.assertFalse(voice.barge_pending)
        voice.silero = lambda window: 0.0
        for _ in range(35):
            if not voice.in_speech:
                break
            voice.step(numpy.zeros(512, dtype=numpy.float32))
        self.assertEqual([e["event"] for e in self.events], ["hearing", "barge", "draft", "hearing", "turn"])
        self.assertEqual(self.events[-1]["text"], "Wait what about tomorrow")

    @unittest.skipIf(numpy is None, "needs numpy")
    def test_interrupt_while_thinking_cancels_stale_answer(self):
        recognizer = FakeRecognizer(["", "ACTUALLY NEVER MIND"], "ACTUALLY NEVER MIND")
        voice = make_voice(recognizer)
        voice.np = numpy
        voice.extend_conversation()
        voice.thinking = True
        # Below-floor noise while thinking does not start a turn.
        voice.silero = lambda window: 0.9
        for _ in range(5):
            voice.step(numpy.full(512, 0.01, dtype=numpy.float32))
        self.assertFalse(voice.in_speech)
        # Normal speech while thinking starts listening and cancels the pending answer once words appear.
        speech = numpy.full(512, 0.05, dtype=numpy.float32)
        for _ in range(4):
            voice.step(speech)
        self.assertIn("barge", [e["event"] for e in self.events])
        # A stale answer arriving for the cancelled turn is dropped instead of spoken.
        voice.command({"cmd": "say", "text": "Old answer", "conversation": "c1"})
        self.assertEqual(voice.speaker.said, [])
        voice.silero = lambda window: 0.0
        for _ in range(35):
            if not voice.in_speech:
                break
            voice.step(numpy.zeros(512, dtype=numpy.float32))
        self.assertEqual([e["event"] for e in self.events], ["hearing", "barge", "hearing", "turn"])
        self.assertEqual(self.events[-1]["text"], "Actually never mind")
        # The replacement answer speaks normally.
        voice.command({"cmd": "say", "text": "New answer", "conversation": "c1"})
        self.assertEqual(voice.speaker.said, [("New answer", "c1")])

    def test_continuation_merges_fragments_into_one_turn(self):
        recognizer = FakeRecognizer(["TELL ME ABOUT THE HISTORY"], "TELL ME ABOUT THE HISTORY")
        voice = make_voice(recognizer)
        voice.extend_conversation()
        # First fragment ends and starts thinking (nothing spoken yet).
        self.speak(voice, 10, 25)
        self.assertEqual(self.events[-1], {"event": "turn", "text": "Tell me about the history"})
        self.assertTrue(voice.thinking)
        # User continues before she starts speaking.
        voice.recognizer = FakeRecognizer(["OF THE ROMAN EMPIRE"], "OF THE ROMAN EMPIRE")
        self.events.clear()
        self.speak(voice, 10, 25)
        barge = next(e for e in self.events if e["event"] == "barge")
        self.assertTrue(barge.get("continuation"))
        self.assertEqual(self.events[-1], {"event": "turn", "text": "Tell me about the history of the roman empire"})

    def test_endpointing_holds_unfinished_phrases_and_shortens_with_draft(self):
        # Complete-looking phrase without draft (< 4 words) finishes around COMPLETE_HOLD_MS (~650 ms, 21 windows).
        voice = make_voice(FakeRecognizer(["WHAT TIME IS"], "WHAT TIME IS"))
        voice.extend_conversation()
        voice.begin([])
        voice.hear(b"", True)
        # "IS" is unfinished, so even after 25 quiet windows (800 ms) it must NOT end the turn!
        for _ in range(25):
            voice.hear(b"", False)
        self.assertTrue(voice.in_speech)
        self.assertEqual([e for e in self.events if e["event"] == "turn"], [])
        # Past INCOMPLETE_HOLD_MS (~1050 ms, 33 windows total) it ends.
        for _ in range(10):
            if not voice.in_speech:
                break
            voice.hear(b"", False)
        self.assertFalse(voice.in_speech)
        self.assertEqual(self.events[-1]["text"], "What time is")

        # Thinking pause "so... um... what I want is" never cuts in prematurely.
        self.events.clear()
        for phrase in ("SO", "SO UM", "SO UM WHAT I WANT IS", "TELL ME ABOUT,"):
            self.assertTrue(voice_module.is_unfinished(phrase), phrase)
        self.assertFalse(voice_module.is_unfinished("WHAT TIME IS IT"))

        # Complete-looking phrase with < 4 words (no draft): stays open at 15 windows (480 ms), ends by 21 windows (672 ms).
        voice2 = make_voice(FakeRecognizer(["WHAT TIME NOW"], "WHAT TIME NOW"))
        voice2.extend_conversation()
        voice2.begin([])
        voice2.hear(b"", True)
        for _ in range(15):
            voice2.hear(b"", False)
        self.assertTrue(voice2.in_speech)
        for _ in range(7):
            if not voice2.in_speech:
                break
            voice2.hear(b"", False)
        self.assertFalse(voice2.in_speech)
        self.assertEqual(self.events[-1]["text"], "What time now")

        # Complete-looking >= 4 word phrase emits draft at ~220 ms and finishes at ~480 ms (15 windows).
        self.events.clear()
        voice3 = make_voice(FakeRecognizer(["WHAT IS THE WEATHER TODAY"], "WHAT IS THE WEATHER TODAY"))
        voice3.extend_conversation()
        voice3.begin([])
        voice3.hear(b"", True)
        for _ in range(7):
            voice3.hear(b"", False)
        self.assertEqual([e["event"] for e in self.events], ["hearing", "draft"])
        self.assertEqual(self.events[-1]["text"], "What is the weather today")
        for _ in range(8):
            if not voice3.in_speech:
                break
            voice3.hear(b"", False)
        self.assertFalse(voice3.in_speech)
        self.assertEqual([e["event"] for e in self.events], ["hearing", "draft", "hearing", "turn"])
        self.assertNotIn("draft_cancel", [e["event"] for e in self.events])

    def test_draft_discards_on_resumed_speech_changed_text_and_caps_at_two(self):
        # Resuming speech cancels draft 1; a second pause starts draft 2; resuming cancels draft 2; a third pause starts NO draft.
        recognizer = FakeRecognizer(["WHAT IS THE WEATHER TODAY"], "WHAT IS THE WEATHER TODAY")
        voice = make_voice(recognizer)
        voice.extend_conversation()
        voice.begin([])
        for attempt in range(3):
            voice.hear(b"", True)
            for _ in range(8):
                voice.hear(b"", False)
            if attempt < 2:
                voice.hear(b"", True)
        drafts = [e for e in self.events if e["event"] == "draft"]
        cancels = [e for e in self.events if e["event"] == "draft_cancel"]
        self.assertEqual(len(drafts), 2)
        self.assertEqual(len(cancels), 2)

        # Changed final transcript cancels the draft before delivering the turn.
        self.events.clear()
        voice2 = make_voice(FakeRecognizer(["WHAT IS THE WEATHER TODAY"], "WHAT IS THE WEATHER TOMORROW"))
        voice2.extend_conversation()
        voice2.begin([])
        voice2.hear(b"", True)
        for _ in range(16):
            if not voice2.in_speech:
                break
            voice2.hear(b"", False)
        self.assertEqual([e["event"] for e in self.events], ["hearing", "draft", "hearing", "draft_cancel", "turn"])

        # Short (< 4 words) or unfinished phrases never emit a draft.
        for phrase in ("HI THERE NOW", "WHAT IS THE WEATHER IN"):
            self.events.clear()
            v = make_voice(FakeRecognizer([phrase], phrase))
            v.extend_conversation()
            v.begin([])
            v.hear(b"", True)
            for _ in range(10):
                v.hear(b"", False)
            self.assertNotIn("draft", [e["event"] for e in self.events], phrase)

    @unittest.skipIf(numpy is None, "needs numpy")
    def test_barge_carries_spoken_so_far_and_fade_out_decays_to_zero(self):
        audio = numpy.ones(24000, dtype=numpy.float32) * 0.5
        faded = voice_module.fade_samples(numpy, audio, 1000, 2400, fade_in=False)
        samples = numpy.frombuffer(faded, dtype=numpy.int16)
        self.assertEqual(len(samples), 2400)
        self.assertGreater(samples[0], 15000)
        self.assertEqual(samples[-1], 0)
        self.assertTrue(bool(numpy.all(samples[:-1] >= samples[1:])))

        # Spoken progress is reported on confirmed barge.
        voice = make_voice(FakeRecognizer(["WAIT WHAT"], "WAIT WHAT"))
        voice.np = numpy
        voice.echo_cancelled = True
        voice.speaker.speaking = True
        voice.speaker.spoken_text = "Paris is the capital. It is known for…"
        loud = numpy.full(512, 0.1, dtype=numpy.float32)
        voice.silero = lambda window: 0.9
        for _ in range(8):
            voice.step(loud)
        barge = next(e for e in self.events if e["event"] == "barge")
        self.assertEqual(barge["spoken"], "Paris is the capital. It is known for…")

    def test_backchannel_while_thinking_does_not_cancel(self):
        voice = make_voice(FakeRecognizer(["OKAY"], "OKAY"))
        voice.extend_conversation()
        voice.thinking = True
        self.speak(voice, 10, 35)
        self.assertTrue(voice.thinking)
        self.assertNotIn("barge", [e["event"] for e in self.events])
        self.assertNotIn("turn", [e["event"] for e in self.events])

    def test_sink_and_source_selection(self):
        import os
        import subprocess
        real_run = voice_module.subprocess.run
        real_env = {k: os.environ.get(k) for k in ("TOHRU_VOICE_SINK", "TOHRU_VOICE_SOURCE")}
        outputs = {
            "sinks": "1\talsa_output.pci\tPipeWire\ts32le\tRUNNING\n2\ttohru-echo-cancel-sink\tPipeWire\tfloat32le\tIDLE\n",
            "sources": "1\talsa_input.usb\tPipeWire\ts32le\tIDLE\n2\ttohru-echo-cancel-sink.monitor\tPipeWire\tfloat32le\tIDLE\n3\ttohru-echo-cancel-source\tPipeWire\tfloat32le\tIDLE\n",
        }
        voice_module.subprocess.run = lambda argv, **kw: subprocess.CompletedProcess(argv, 0, outputs[argv[-1]], "")
        try:
            os.environ.pop("TOHRU_VOICE_SINK", None)
            os.environ.pop("TOHRU_VOICE_SOURCE", None)
            speaker = object.__new__(voice_module.Speaker)
            voice = make_voice(FakeRecognizer([""], ""))
            self.assertEqual(speaker.sink(), "tohru-echo-cancel-sink")
            self.assertEqual(voice.source(), "tohru-echo-cancel-source")
            os.environ["TOHRU_VOICE_SINK"] = "custom-sink"
            os.environ["TOHRU_VOICE_SOURCE"] = "custom-source"
            self.assertEqual(speaker.sink(), "custom-sink")
            self.assertEqual(voice.source(), "custom-source")
            os.environ.pop("TOHRU_VOICE_SINK", None)
            outputs["sinks"] = "1\talsa_output.pci\tPipeWire\ts32le\tRUNNING\n"
            self.assertIsNone(speaker.sink())
        finally:
            voice_module.subprocess.run = real_run
            for k, v in real_env.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

    def test_speaker_pause_and_resume_events(self):
        import threading
        speaker = object.__new__(voice_module.Speaker)
        speaker.cond = threading.Condition()
        speaker.stop = threading.Event()
        speaker.speaking = True
        speaker.paused = False
        speaker.conversation = "c1"
        speaker.level = 0.3
        speaker.recent = [(1.0, 0.3)]
        speaker.pause()
        self.assertTrue(speaker.paused)
        self.assertEqual(speaker.level, 0.0)
        self.assertEqual(self.events, [
            {"event": "mouth", "level": 0, "viseme": voice_module.REST},
            {"event": "speaking", "on": False, "paused": True, "conversation": "c1"},
        ])
        speaker.resume()
        self.assertFalse(speaker.paused)
        self.assertEqual(self.events[-1], {"event": "speaking", "on": True, "conversation": "c1"})

    def test_speech_outside_a_conversation_is_not_shown_as_heard(self):
        voice = make_voice(FakeRecognizer(["WHAT"], "WHAT"))
        self.speak(voice, 20, 40)
        self.assertNotIn("hearing", [e["event"] for e in self.events])

    def test_pause_follows_the_session_lock_file(self):
        import fcntl
        voice = make_voice(FakeRecognizer([""], ""))
        voice.silero = type("S", (), {"reset": lambda self: None})()
        voice.capture = None
        monitor = object.__new__(voice_module.Monitor)
        monitor.voice = voice
        monitor.locked = False
        voice.monitor = monitor
        with tempfile.NamedTemporaryFile() as lock:
            monitor.lock_file = lock.name
            self.assertFalse(monitor.is_locked())
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.assertTrue(monitor.is_locked())
            monitor.locked = True
            voice.paused_changed()
            self.assertEqual(voice.paused_reason, "locked")
            self.assertEqual(voice.speaker.hushed, 1)
            self.assertEqual(self.events[-1], {"event": "paused", "reason": "locked"})
            fcntl.flock(lock.fileno(), fcntl.LOCK_UN)
            self.assertFalse(monitor.is_locked())
            monitor.locked = False
            voice.paused_changed()
            self.assertIsNone(voice.paused_reason)
            self.assertEqual(self.events[-1], {"event": "paused", "reason": ""})


class VisemeTests(unittest.TestCase):
    def test_phonemes_map_to_the_eight_mouths(self):
        m = voice_module
        self.assertEqual([v for v, _ in m.visemes("mˈɑːm")], [m.MBP, m.AA, m.MBP])
        self.assertEqual([v for v, _ in m.visemes("fˈiː")], [m.FV, m.EE])
        self.assertEqual([v for v, _ in m.visemes("hˈO, wˈW")], [m.EH, m.OH, m.REST, m.OO, m.AA])
        self.assertEqual(m.visemes("  "), [])

    def test_the_timeline_fills_the_clip_and_drops_flickers(self):
        m = voice_module
        sounds = [(m.AA, 1.0), (m.EE, 0.05), (m.OH, 1.0), (m.MBP, 0.05), (m.AA, 1.0)]
        spans = m.viseme_timeline(sounds, None, 1.0)
        self.assertAlmostEqual(spans[0][0], 0.0)
        self.assertAlmostEqual(spans[-1][1], 1.0)
        self.assertNotIn(m.EE, [s[2] for s in spans])
        self.assertIn(m.MBP, [s[2] for s in spans])
        for a, b in zip(spans, spans[1:]):
            self.assertAlmostEqual(a[1], b[0])
        self.assertEqual(m.viseme_at(spans, 0.1), m.AA)
        self.assertEqual(m.viseme_at(spans, 2.0), m.REST)

    def test_model_durations_place_the_mouths(self):
        m = voice_module
        spans = m.viseme_timeline([(m.AA, 1.0), (m.OO, 1.0)], [3.0, 1.0], 2.0)
        self.assertEqual([(round(a, 2), round(b, 2), v) for a, b, v in spans], [(0.0, 1.5, m.AA), (1.5, 2.0, m.OO)])

    @unittest.skipIf(numpy is None, "numpy is not installed")
    def test_kokoro_durations_include_the_pads(self):
        m = voice_module
        audio = numpy.zeros(m.KOKORO_RATE, dtype=numpy.float32)
        spans = m.kokoro_timeline(numpy, audio, "mɑ", [1.0, 1.0, 1.0, 1.0])
        self.assertEqual([v for _, _, v in spans], [m.REST, m.MBP, m.AA, m.REST])
        self.assertAlmostEqual(spans[1][0], 0.25)


class ModelPathTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        self.vad = self.touch("silero-vad-v6.2.3.onnx")
        self.turn = self.touch("smart-turn-v3.2-cpu.onnx")
        self.asr = self.transducer("asr")
        self.kws = self.transducer("kws")

    def tearDown(self):
        self.dir.cleanup()

    def touch(self, name):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"")
        return str(path)

    def transducer(self, name):
        for file in ("tokens.txt", "encoder-epoch-99-avg-1.int8.onnx", "decoder-epoch-99-avg-1.onnx", "joiner-epoch-99-avg-1.int8.onnx"):
            self.touch(f"{name}/{file}")
        return str(self.root / name)

    def test_complete_paths_pass(self):
        self.assertEqual(voice_module.check_models(self.vad, self.asr, self.turn, "kws", self.kws), ([], []))
        self.assertEqual(voice_module.check_models(self.vad, self.asr, "", "asr", ""), ([], []))

    def test_unset_required_paths_name_each_setting_and_file(self):
        errors, warnings = voice_module.check_models("", "", "", "kws", "")
        self.assertEqual(len(errors), 3)
        self.assertIn("assistant.vad_model is not set", errors[0])
        self.assertIn(".onnx", errors[0])
        self.assertIn("assistant.asr_model is not set", errors[1])
        self.assertIn("tokens.txt, encoder*.onnx, decoder*.onnx, joiner*.onnx", errors[1])
        self.assertIn("assistant.kws_model is not set", errors[2])
        self.assertEqual(warnings, [])

    def test_kws_folder_is_only_needed_for_kws_wake(self):
        self.assertEqual(voice_module.check_models(self.vad, self.asr, "", "asr", "/nowhere"), ([], []))

    def test_invalid_paths_are_reported(self):
        (self.root / "asr/joiner-epoch-99-avg-1.int8.onnx").unlink()
        (self.root / "asr/tokens.txt").unlink()
        errors, _ = voice_module.check_models(self.asr, self.asr, "", "asr", "")
        self.assertIn(f"assistant.vad_model: no .onnx file at {self.asr}", errors[0])
        self.assertIn(f"assistant.asr_model: {self.asr} has no tokens.txt, joiner*.onnx", errors[1])
        errors, _ = voice_module.check_models(self.vad, str(self.root / "missing"), "", "asr", "")
        self.assertIn("missing is not a folder", errors[0])

    def test_bad_turn_model_warns_and_falls_back_to_silence(self):
        errors, warnings = voice_module.check_models(self.vad, self.asr, str(self.root / "turn.onnx"), "asr", "")
        self.assertEqual(errors, [])
        self.assertEqual(len(warnings), 1)
        self.assertIn("assistant.turn_model", warnings[0])
        self.assertIn("silence only", warnings[0])

    def test_transducer_picks_int8_and_rejects_incomplete_folders(self):
        files = voice_module.transducer(Path(self.asr))
        self.assertTrue(files["encoder"].endswith("encoder-epoch-99-avg-1.int8.onnx"))
        (self.root / "asr/tokens.txt").unlink()
        with self.assertRaises(FileNotFoundError):
            voice_module.transducer(Path(self.asr))

    def test_kokoro_needs_an_explicit_folder(self):
        with self.assertRaisesRegex(FileNotFoundError, "assistant.tts_model is not set"):
            voice_module.kokoro_files({"model": ""})
        with self.assertRaisesRegex(FileNotFoundError, "has no model.onnx and tokenizer.json"):
            voice_module.kokoro_files({"model": self.asr})
        self.touch("kokoro/model.onnx")
        self.touch("kokoro/tokenizer.json")
        self.touch("kokoro/voices/af_heart.bin")
        model, vocab, pack = voice_module.kokoro_files({"model": str(self.root / "kokoro"), "voicepack": "af_heart"})
        self.assertEqual((model.name, vocab.name, pack.name), ("model.onnx", "tokenizer.json", "af_heart.bin"))

    def test_missing_kokoro_warns_through_the_given_callback(self):
        notes = []
        engine = voice_module.make_engine({"engine": "kokoro", "model": ""}, notes.append)
        self.assertIsInstance(engine, voice_module.Espeak)
        self.assertIn("assistant.tts_model is not set", notes[0])
        self.assertIn("using espeak-ng", notes[0])


if __name__ == "__main__":
    unittest.main()
