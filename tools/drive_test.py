"""Unit tests for tools/drive.py. Run: python3 -m unittest tools/drive_test.py"""

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import drive  # noqa: E402


def dump(tabs, in_flight=0, quiet_ms=900, pending_writes=0):
    return {
        "tabs": tabs,
        "in_flight": in_flight,
        "pending_writes": pending_writes,
        "quiet_ms": quiet_ms,
    }


class SettleTest(unittest.TestCase):
    def test_settled_needs_idle_tabs_no_calls_and_quiet(self):
        idle = [{"busy": False}]
        self.assertTrue(drive.settled(dump(idle)))
        self.assertFalse(drive.settled(dump([{"busy": True}])))
        self.assertFalse(drive.settled(dump(idle, in_flight=1)))
        self.assertFalse(drive.settled(dump(idle, pending_writes=1)))
        self.assertFalse(drive.settled(dump(idle, quiet_ms=499)))
        self.assertTrue(drive.settled(dump(idle, quiet_ms=500)))
        self.assertFalse(drive.settled(dump(idle, quiet_ms=None)))


class RedactTest(unittest.TestCase):
    def test_masks_secret_lines_and_the_username(self):
        lines = [
            "plain line",
            "got access_token abc",
            "Authorization: Bearer x",
            "auth failed",
            "hello someuser",
        ]
        out = drive.redact(lines, "someuser")
        self.assertEqual(out[0], "plain line")
        self.assertTrue(all(line == "[redacted]" for line in out[1:]))

    def test_no_username_masks_only_secret_words(self):
        self.assertEqual(drive.redact(["a b"], None), ["a b"])


class ScrubTest(unittest.TestCase):
    def test_redact_masks_spotify_ids(self):
        out = drive.redact(["open spotify:track:4uLU6hMCjMI75M1A2tKUQC now"], None)
        self.assertEqual(out, ["open spotify:track:[id] now"])

    def test_redact_masks_bare_ids_and_drops_ipc_status_lines(self):
        lines = [
            'url: "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC"',
            '[ncspot::ipc] [DEBUG] IPC Status update: Status { title: "Some Song" }',
        ]
        out = drive.redact(lines, None)
        self.assertEqual(out[0], 'url: "https://open.spotify.com/track/[id]"')
        self.assertEqual(out[1], "[ipc status update]")

    def test_status_segments_keep_only_bracketed_segments(self):
        pane = ["  Some Artist - Some Song   3:01 |", " >> Song  [rate limited: shared 24s] [R1] 0:02"]
        self.assertEqual(drive.status_segments(pane), ["[rate limited: shared 24s] [R1]"])
        self.assertEqual(drive.status_segments(["no brackets"]), [])

    def test_scrub_dump_drops_the_title(self):
        self.assertEqual(drive.scrub_dump({"title": "Library of x", "seq": 1}), {"seq": 1})

    def test_library_client_follows_the_personal_client_id(self):
        self.assertEqual(drive.library_client({}), "shared")
        self.assertEqual(drive.library_client({"NCSPOT_CLIENT_ID": " "}), "shared")
        self.assertEqual(drive.library_client({"NCSPOT_CLIENT_ID": "ab" * 16}), "personal")


class ConfigTest(unittest.TestCase):
    def test_renders_scalars_and_one_level_tables(self):
        text = drive.render_config(
            {"internal_api": False, "internal_api_areas": {"metadata": False}, "x": "y"}
        )
        self.assertIn("internal_api = false\n", text)
        self.assertIn('x = "y"\n', text)
        self.assertIn("[internal_api_areas]\nmetadata = false\n", text)
        # Tables come after top-level keys, as TOML requires.
        self.assertLess(text.index("x ="), text.index("[internal_api_areas]"))


class ExitCodeTest(unittest.TestCase):
    def test_maps_results_to_exit_codes(self):
        self.assertEqual(drive.exit_code(["PASS", "PASS"]), 0)
        self.assertEqual(drive.exit_code(["PASS", "FAIL", "BLOCKED"]), 1)
        self.assertEqual(drive.exit_code(["PASS", "BLOCKED"]), 3)
        self.assertEqual(drive.exit_code([]), 0)


class WriteBackTest(unittest.TestCase):
    OLD = b'{"access_token": "a"}'
    NEW = b'{"access_token": "b"}'

    def test_unchanged_temp_is_not_written(self):
        self.assertFalse(drive.should_write_back(self.OLD, self.OLD, self.OLD))

    def test_changed_temp_with_unchanged_real_is_written(self):
        self.assertTrue(drive.should_write_back(self.OLD, self.NEW, self.OLD))

    def test_real_changed_meanwhile_is_left_alone(self):
        self.assertFalse(drive.should_write_back(self.OLD, self.NEW, b'{"access_token": "c"}'))

    def test_unusable_temp_is_not_written(self):
        self.assertFalse(drive.should_write_back(self.OLD, None, self.OLD))
        self.assertFalse(drive.should_write_back(self.OLD, b"not json", self.OLD))
        self.assertFalse(drive.should_write_back(self.OLD, b'{"access_token": ""}', self.OLD))
        self.assertFalse(drive.should_write_back(self.OLD, b"{}", self.OLD))

    def test_missing_real_is_not_written(self):
        self.assertFalse(drive.should_write_back(self.OLD, self.NEW, None))


class TokenFileTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def path(self, name):
        return os.path.join(self.dir.name, name)

    def test_copy_returns_exactly_the_bytes_written(self):
        src, dst = self.path("real.json"), self.path("copy.json")
        with open(src, "wb") as f:
            f.write(b'{"access_token": "a"}')
        copied = drive.copy_token(src, dst)
        with open(dst, "rb") as f:
            self.assertEqual(f.read(), copied)
        self.assertEqual(copied, b'{"access_token": "a"}')
        self.assertEqual(os.stat(dst).st_mode & 0o777, 0o600)

    def test_atomic_write_replaces_the_file_private(self):
        real = self.path("real.json")
        with open(real, "wb") as f:
            f.write(b"old")
        drive.write_atomic(real, b"new")
        with open(real, "rb") as f:
            self.assertEqual(f.read(), b"new")
        self.assertEqual(os.stat(real).st_mode & 0o777, 0o600)
        self.assertFalse(os.path.exists(real + ".tmp"))

    def test_failed_atomic_write_leaves_no_part_file(self):
        real = self.path("real.json")
        os.mkdir(real)  # os.replace onto a directory fails
        with self.assertRaises(OSError):
            drive.write_atomic(real, b"new")
        self.assertFalse(os.path.exists(real + ".tmp"))


class AreaDeltaTest(unittest.TestCase):
    def test_area_delta_subtracts_the_baseline_and_sums_web_calls(self):
        def d(lib_ok, lib_fb, meta_ok, shared, personal):
            return {
                "areas": {
                    "library": {"internal_ok": lib_ok, "fallback": lib_fb},
                    "metadata": {"internal_ok": meta_ok, "fallback": 0},
                },
                "web": {"shared": shared, "personal": personal},
            }

        class Inst:
            base_dump = d(10, 1, 5, 2, 3)

        now = d(14, 2, 9, 3, 7)
        self.assertEqual(
            drive.area_delta(Inst, now, "library"), {"internal_ok": 4, "fallback": 1, "web": 5}
        )
        self.assertEqual(drive.delta(Inst, now), {"internal_ok": 4, "fallback": 0, "web": 5})


class LibraryDeltaTest(unittest.TestCase):
    @staticmethod
    def run_with(ok, fallback):
        def d(lib_ok, lib_fb):
            return {
                "areas": {
                    "library": {"internal_ok": lib_ok, "fallback": lib_fb, "latched": False}
                },
                "web": {"shared": 0, "personal": 0},
                "throttled": {},
            }

        class Inst:
            base_dump = d(10, 0)

            def send(self, cmd):
                pass

            def wait_library(self, timeout):
                return d(10 + ok, fallback)

        class Args:
            settle_timeout = 1

        drive.sc_library_delta(Inst(), Args())

    def test_a_small_internal_delta_passes(self):
        self.run_with(4, 0)

    def test_a_fallback_no_request_or_a_repage_fails(self):
        for ok, fallback in [(0, 1), (2, 1), (0, 0), (7, 0)]:
            with self.assertRaises(drive.Fail, msg=(ok, fallback)):
                self.run_with(ok, fallback)


class LibraryWriteTest(unittest.TestCase):
    URIS = [
        f"spotify:{k}:{i}"
        for k, i in [("track", drive.TRACK), ("artist", drive.ARTIST), ("show", drive.SHOW)]
    ]

    class Args:
        settle_timeout = 1

    @staticmethod
    def library_write_inst(ok=6, fallback=0, errors=()):
        """A fake instance; `errors` is the error of each successive save/unsave dump."""

        def d(lib_ok, lib_fb, error=None):
            return {
                "error": error,
                "areas": {
                    "library": {"internal_ok": lib_ok, "fallback": lib_fb, "latched": False}
                },
                "web": {"shared": 0, "personal": 0},
                "throttled": {},
            }

        errs = list(errors)

        class Inst:
            base_dump = d(10, 0)
            sent = []

            def send(self, cmd):
                self.sent.append(cmd)

            def wait_library(self, timeout, blocked_on=()):
                return d(10, 0)

            def wait_settled(self, timeout):
                return d(10 + ok, fallback, errs.pop(0) if errs else None)

        return Inst()

    @classmethod
    def run_with(cls, ok, fallback, errors=()):
        """Run the scenario against a fake instance. Returns the lines sent."""
        inst = cls.library_write_inst(ok, fallback, errors)
        drive.sc_library_write(inst, cls.Args())
        return inst.sent

    def test_library_write_during_sync_order(self):
        inst = self.library_write_inst()
        drive.sc_library_write_during_sync(inst, self.Args())
        sent = inst.sent
        first = next(i for i, c in enumerate(sent) if c.startswith("harness save"))
        self.assertIn("update", sent[:first])
        self.assertTrue(sent[first + 1].startswith("harness unsave"))

    def test_saves_then_removes_each_item(self):
        sent = self.run_with(6, 0)
        expected = ["update"]
        for u in self.URIS:
            expected += [f"harness save {u}", f"harness unsave {u}"]
        self.assertEqual(sent, expected)

    def test_a_fallback_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(6, 1)

    def test_an_error_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(6, 0, errors=["harness save: library unchanged"])

    def test_every_item_skipped_is_blocked(self):
        with self.assertRaises(drive.Blocked):
            self.run_with(0, 0, errors=["harness save: already in the library"] * 3)

    def test_an_item_already_saved_is_skipped_not_removed(self):
        sent = self.run_with(4, 0, errors=["harness save: already in the library"])
        self.assertNotIn(f"harness unsave {self.URIS[0]}", sent)
        self.assertIn(f"harness unsave {self.URIS[1]}", sent)


class CleanupTest(unittest.TestCase):
    def test_parses_the_two_subcommands(self):
        a = drive.parse_cleanup(["test-playlists"])
        self.assertEqual(a.command, "test-playlists")
        b = drive.parse_cleanup(["unfollow", "2", "--no-build"])
        self.assertEqual((b.command, b.n, b.no_build), ("unfollow", 2, True))

    def test_rejects_a_missing_or_bad_number(self):
        for argv in (["unfollow"], ["unfollow", "x"], ["unfollow", "0"], ["unfollow", "-1"]):
            with self.assertRaises(SystemExit):
                drive.parse_cleanup(argv)

    def test_subcommands_are_not_scenarios(self):
        for name in drive.CLEANUP_COMMANDS:
            self.assertNotIn(name, drive.SCENARIOS)

    def test_a_scenario_list_naming_a_subcommand_is_unknown(self):
        self.assertEqual(drive.main(["album", "test-playlists", "--no-build"]), 2)

    def test_formats_numbered_names_only(self):
        self.assertEqual(drive.format_test_playlists(["ncspot-test-1"]), ["1 ncspot-test-1"])
        self.assertEqual(
            drive.format_test_playlists(["ncspot-test-1", "(unknown)"]),
            ["1 ncspot-test-1", "2 (unknown)"],
        )
        self.assertEqual(drive.format_test_playlists([]), [])

    def test_the_child_gets_the_real_list_path(self):
        self.assertEqual(
            drive.TEST_PLAYLISTS_FILE, os.path.join(drive.REAL_CACHE, "test-playlists.txt")
        )


class PlaylistsScenarioTest(unittest.TestCase):
    @staticmethod
    def d(fallback=0, latched=False, stored=9, layout=8, rows=9, base=False, tab=True, last_len=3, error=None, order=None, renamed=None):
        return {
            "areas": {
                "playlists": {"internal_ok": 0 if base else 4, "fallback": fallback, "latched": latched}
            },
            "web": {"shared": 0, "personal": 0},
            "throttled": {},
            "playlists": {"stored": stored, "layout": layout, "last_test_len": last_len,
                          "last_test_order": order, "last_test_renamed": renamed},
            "error": error,
            "tabs": [{"name": "Playlists", "rows": rows}] if tab else [{"name": "Albums", "rows": 3}],
        }

    # The test playlist's length after the create and after each write command.
    WRITE_LENS = (0, 2, 1, 3)
    # The order after each of the three moves.
    WRITE_ORDERS = ("BAC", "BCA", "CAB", "ABC")

    def run_with(self, func, lens=None, orders=None, **kw):
        base = self.d(base=True)
        dump = self.d(**kw)
        if lens is None and func is drive.sc_playlist_write:
            lens = self.WRITE_LENS
        # One dump per settle, in order; past the end (or without lens) the same dump.
        settled = [self.d(**{**kw, "last_len": n}) for n in lens or ()]
        if func is drive.sc_playlist_write:
            orders = orders or self.WRITE_ORDERS
            settled += [self.d(**{**kw, "order": o}) for o in orders]
            settled.append(self.d(**{**kw, "renamed": True}))

        class Inst:
            base_dump = base

            def send(self, cmd):
                pass

            def wait_library(self, timeout, blocked_on=()):
                # The write scenario measures from the dump taken when the library is done.
                return base if func is drive.sc_playlist_write else dump

            def wait_settled(self, timeout):
                return settled.pop(0) if settled else dump

        class Args:
            settle_timeout = 1

        func(Inst(), Args())

    def test_playlists_passes(self):
        self.run_with(drive.sc_playlists)

    def test_playlists_fallback_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlists, fallback=1)

    def test_playlists_latched_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlists, latched=True)

    def test_playlists_missing_layout_or_unstored_fails(self):
        for kw in [{"layout": 0}, {"stored": 5, "layout": 8}]:
            with self.assertRaises(drive.Fail, msg=kw):
                self.run_with(drive.sc_playlists, **kw)

    def test_area_off_passes(self):
        self.run_with(drive.sc_playlists_area_off)

    def test_area_off_stale_count_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlists_area_off, stored=4, rows=4)

    def test_area_off_row_mismatch_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlists_area_off, rows=12)

    def test_area_off_missing_tab_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlists_area_off, tab=False)

    def test_playlist_write_passes(self):
        self.run_with(drive.sc_playlist_write)

    def test_playlist_write_fallback_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlist_write, fallback=1)

    def test_playlist_write_latched_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlist_write, latched=True)

    def test_playlist_write_wrong_length_fails(self):
        for lens in ((0, 1, 1, 3), (0, 2, 2, 3), (0, 2, 1, 2), (0, 2, 1, None)):
            with self.assertRaises(drive.Fail, msg=lens):
                self.run_with(drive.sc_playlist_write, lens=lens)

    def test_playlist_write_wrong_order_or_unrenamed_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlist_write, orders=("BAC", "ABC", "CAB"))

    def test_playlist_write_throttled_is_blocked(self):
        original = self.d

        def throttled(**kw):
            d = original(**kw)
            d["throttled"] = {"internal": True}
            return d

        self.d = throttled
        with self.assertRaises(drive.Blocked):
            self.run_with(drive.sc_playlist_write, error="boom")

    def test_playlist_write_error_fails(self):
        with self.assertRaises(drive.Fail):
            self.run_with(drive.sc_playlist_write, error="boom")


class ItemWriteTest(unittest.TestCase):
    def test_a_running_write_is_not_settled(self):
        d = dump([], quiet_ms=900)
        self.assertTrue(drive.settled(d))
        d["item_write"] = "running"
        self.assertFalse(drive.settled(d))

    def test_a_failed_write_reads_as_the_commands_error(self):
        d = drive.with_item_error({"item_write": "harness save: library unchanged", "error": None})
        self.assertEqual(d["error"], "harness save: library unchanged")
        for state in ("ok", "running", None):
            self.assertIsNone(drive.with_item_error({"item_write": state, "error": None})["error"])


class WaitWhileAnsweringTest(unittest.TestCase):
    """A timeout alone never fails a wait while ncspot keeps answering."""

    class Fake:
        def __init__(self, answers):
            self.answers = list(answers)

        def dump(self):
            return self.answers.pop(0)

    def setUp(self):
        self.ceiling = drive.ANSWERING_CEILING
        self.addCleanup(setattr, drive, "ANSWERING_CEILING", self.ceiling)
        self.busy = dump([], in_flight=1)
        self.throttled = {"internal": False}
        self.busy["throttled"] = self.throttled
        self.done = dump([], quiet_ms=900)
        self.done["library_done"] = True

    def test_wait_settled_outlasts_its_timeout(self):
        fake = self.Fake([self.busy, self.busy, self.done])
        self.assertIs(drive.Instance.wait_settled(fake, 0), self.done)

    def test_wait_library_outlasts_its_timeout(self):
        fake = self.Fake([self.busy, self.busy, self.done])
        self.assertIs(drive.Instance.wait_library(fake, 0), self.done)

    def test_a_rate_limit_past_the_timeout_is_still_blocked(self):
        limited = dict(self.busy, throttled={"internal": True})
        with self.assertRaises(drive.Blocked):
            drive.Instance.wait_library(self.Fake([limited]), 0, blocked_on=["internal"])

    def test_the_ceiling_ends_the_wait(self):
        drive.ANSWERING_CEILING = -1.0
        with self.assertRaises(drive.Fail):
            drive.Instance.wait_settled(self.Fake([self.busy]), 0)
        with self.assertRaises(drive.Fail):
            drive.Instance.wait_library(self.Fake([self.busy]), 0)


if __name__ == "__main__":
    unittest.main()
