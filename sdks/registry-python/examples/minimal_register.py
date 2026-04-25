import asyncio

from rinfra_registry_sdk import Endpoint, Registration, RegistryClientConfig, RegistryClient


async def main() -> None:
    config = RegistryClientConfig(
        main_address="127.0.0.1:7946",
        cluster_token="change-me-in-production",
    )
    registration = Registration(
        node_id="python-order-api-1",
        endpoints=[Endpoint(protocol="http", address="10.0.1.24:8080")],
        metadata={
            "meta.schema": "rinfra.meta.v1",
            "service.name": "order-api",
            "service.instance_id": "python-order-api-1",
            "service.version": "0.1.0",
            "service.env": "dev",
        },
    )
    client = RegistryClient(config, registration)
    await client.start()
    try:
        await asyncio.sleep(60)
    finally:
        await client.stop()


if __name__ == "__main__":
    asyncio.run(main())
