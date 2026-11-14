import importlib.util
from pathlib import Path
import unittest
from unittest.mock import Mock, patch


spec = importlib.util.spec_from_file_location(
    "muse_host_probe", Path(__file__).with_name("muse-host-probe.py")
)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class FakeHost:
    def __init__(self, account, subscription):
        self.account = account
        self.subscription = subscription
        self.calls = []

    def rpc(self, identity, method, params):
        self.calls.append(method)
        if method == "account/read":
            return {"state": self.account, "credentialRequired": True}
        if method == "usage/refresh":
            return self.subscription
        if method == "model/complete":
            return {"status": "accepted", "commandId": params["commandId"]}
        if method == "usage/read":
            return {}
        raise AssertionError("Unexpected RPC method")

    def wait_for(self, predicate, timeout):
        completion = {
            "method": "model/completed",
            "params": {"commandId": "test-id", "outcome": "completed", "text": "MELLY_PROBE_OK"},
        }
        assert predicate(completion)
        return completion


class SubscriptionGateTests(unittest.TestCase):
    def test_requires_browser_account_before_refresh_or_inference(self):
        for account in ("apiKey", "envKey", "unknown"):
            with self.subTest(account=account):
                host = FakeHost(account, {"subscriptionActive": True})
                with self.assertRaisesRegex(RuntimeError, "browser sign-in"):
                    probe.subscription_probe(host, "test-id", infer=True)
                self.assertEqual(host.calls, ["account/read"])

    def test_inaccessible_login_stops_before_refresh_or_inference(self):
        host = FakeHost("loggedOut", {"subscriptionActive": True})
        with self.assertRaisesRegex(RuntimeError, "could not access an existing Muse login"):
            probe.subscription_probe(host, "test-id", infer=True)
        self.assertEqual(host.calls, ["account/read"])

    def test_inactive_or_unknown_entitlement_never_sends_prompt(self):
        for usage in ({}, {"subscriptionActive": False}, {"subscriptionActive": "true"}):
            with self.subTest(usage=usage):
                host = FakeHost("accountLogin", usage)
                with self.assertRaisesRegex(RuntimeError, "active Muse Code plan"):
                    probe.subscription_probe(host, "test-id", infer=True)
                self.assertEqual(host.calls, ["account/read", "usage/refresh"])

    def test_status_only_never_sends_prompt_even_with_active_plan(self):
        host = FakeHost("accountLogin", {"subscriptionActive": True})
        self.assertFalse(probe.subscription_probe(host, "test-id", infer=False))
        self.assertEqual(host.calls, ["account/read", "usage/refresh"])

    def test_active_plan_allows_one_stateless_completion_only(self):
        host = FakeHost("accountLogin", {"subscriptionActive": True})
        self.assertTrue(probe.subscription_probe(host, "test-id", infer=True))
        self.assertEqual(
            host.calls,
            [
                "account/read",
                "usage/refresh",
                "account/read",
                "model/complete",
                "account/read",
                "usage/read",
            ],
        )


class IsolatedLoginTests(unittest.TestCase):
    def test_browser_login_rejects_an_unrecognized_destination(self):
        host = Mock()
        host.rpc.side_effect = [
            {"state": "loggedOut"},
            {"verificationUrl": "https://auth.meta.com.example.org/device", "userCode": "ABCD-1234"},
        ]
        with patch.object(probe.subprocess, "run") as open_browser:
            with self.assertRaisesRegex(RuntimeError, "unrecognized browser approval destination"):
                probe.login_in_browser(host)
        open_browser.assert_not_called()

    def test_fresh_host_requests_only_device_login(self):
        host = Mock()
        host.rpc.side_effect = [
            {"state": "loggedOut"},
            {"verificationUrl": "https://auth.meta.com/device?user_code=ABCD-1234", "userCode": "ABCD-1234"},
        ]
        host.wait_for.return_value = {"params": {"outcome": "granted"}}
        with patch.object(probe.subprocess, "run", return_value=Mock(returncode=0)) as open_browser:
            probe.login_in_browser(host)
        host.rpc.assert_called_with(21, "account/loginStart", {"type": "deviceCode"})
        open_browser.assert_called_once()


if __name__ == "__main__":
    unittest.main()
