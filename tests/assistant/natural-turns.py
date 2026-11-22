#!/usr/bin/env python3
"""Semantic freshness and delivery-evidence regressions, no models or accounts."""
import runpy
import unittest
import queue
import threading
from pathlib import Path

fixture = runpy.run_path(str(Path(__file__).with_name('voice.py')))
v = fixture['voice_module']
make = fixture['make_voice']
Recognizer = fixture['FakeRecognizer']


class SemanticTests(unittest.TestCase):
    def setUp(self):
        self.old = v.SEMANTIC
        v.SEMANTIC = True
        self.events = []
        v.emit = lambda event, **kw: self.events.append(dict(event=event, **kw))

    def tearDown(self):
        v.SEMANTIC = self.old

    def voice(self, text='Maybe a map would work'):
        voice = make(Recognizer([text], text))
        voice.conversation_until = float('inf')
        voice.begin([])
        voice.maybe_draft(text)
        return voice

    def decide(self, voice, text, token=None):
        voice.command(dict(cmd='semantic', decision='respond', text=text,
                           token=token or voice.semantic_token))

    def test_resumed_speech_invalidates_authority(self):
        voice = self.voice()
        token = voice.semantic_token
        voice.hear(b'', True)
        self.decide(voice, 'Maybe a map would work', token)
        self.assertIsNone(voice.semantic_decision)

    def test_wrong_text_and_token_cannot_authorize(self):
        voice = self.voice()
        self.decide(voice, 'Something else')
        self.assertIsNone(voice.semantic_decision)
        self.decide(voice, 'Maybe a map would work', 'old')
        self.assertIsNone(voice.semantic_decision)

    def test_final_revision_abstains(self):
        voice = self.voice()
        self.decide(voice, 'Maybe a map would work')
        voice.recognizer.final = 'Actually hold on'
        voice.end_turn()
        self.assertFalse(any(e['event'] == 'turn' for e in self.events))
        self.assertEqual(voice.pending_continuation, 'Actually hold on')

    def test_explicit_handoff_after_wait(self):
        voice = self.voice('Hold on let me think')
        for _ in range(126):
            if voice.in_speech: voice.hear(b'', False)
        self.assertFalse(voice.in_speech)
        voice.recognizer = Recognizer(['Go ahead'], 'Go ahead')
        voice.begin([])
        for _ in range(24):
            if voice.in_speech: voice.hear(b'', False)
        self.assertEqual(sum(e['event'] == 'turn' for e in self.events), 1)

    def test_end_invalidates_capture(self):
        voice = self.voice()
        token = voice.semantic_token
        voice.command(dict(cmd='end'))
        self.decide(voice, 'Maybe a map would work', token)
        self.assertIsNone(voice.semantic_decision)

    def test_label_content_is_not_clear_handoff(self):
        self.assertFalse(v.explicit_handoff('RESPOND'))
        self.assertFalse(v.explicit_handoff('The string says WAIT'))

    def test_wake_only_still_greets(self):
        voice = make(Recognizer(['Tohru'], 'Tohru'))
        voice.begin([])
        for _ in range(24):
            if voice.in_speech: voice.hear(b'', False)
        self.assertTrue(any(e['event'] == 'wake' for e in self.events))

    def test_queued_generation_cannot_inherit_old_playback(self):
        speaker = object.__new__(v.Speaker)
        speaker.cond = threading.Condition()
        speaker.stop = threading.Event()
        speaker.player = None
        speaker.voice = type('Voice', (), {'speech_generation': 2})()
        speaker.jobs = queue.Queue(maxsize=1)
        speaker.spoken_parts = ['Old completed reply.']
        speaker.current_part = 'Old partial reply.'
        speaker.current_fraction = .5
        speaker.say('New reply awaiting synthesis.', 'new-conversation')
        evidence = speaker.spoken_so_far()
        self.assertNotIn('Old', evidence)
        self.assertNotIn('New reply', evidence)
        self.assertIn('none', evidence)


if __name__ == '__main__': unittest.main()
