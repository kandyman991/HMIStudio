# Development and simulation policy

Development and isolated simulation do not require production approval for PLC reads/writes, operator commands, setpoints, alarms, recipes, runtime tests or unsigned local development exports as capabilities become available. Do not introduce a read-only simulation gate merely because the product is under development.

Actual machinery, physical-device commissioning, release signing, deployment trust and third-party licensing are separate concerns. Do not bypass real deployment identity, pairing or package verification for production. Simulated targets must remain clearly distinguished from actual machinery; default examples use synthetic data.

The initial desktop shell does not implement PLC connectivity or production runtime deployment. No physical hardware acceptance is claimed. Test using synthetic fixtures and isolated simulators first; actual machine acceptance must be separately documented.
