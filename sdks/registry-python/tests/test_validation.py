import unittest

from rinfra_registry_sdk import Endpoint, Registration, RegistryClientConfig, RegistryClient


class ValidationTests(unittest.TestCase):
    def test_empty_endpoints_rejected(self) -> None:
        cfg = RegistryClientConfig(main_address="127.0.0.1:7946", cluster_token="token")
        reg = Registration(node_id="n1", endpoints=[], metadata={})
        with self.assertRaises(ValueError):
            RegistryClient(cfg, reg)


if __name__ == "__main__":
    unittest.main()
