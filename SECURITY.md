# Security

Report security problems privately through GitHub's security advisory form for this repository.
Do not open a public issue for a vulnerability that could affect plugin users.

SoundGIF payloads are untrusted media. Parsers must validate lengths before allocation or slicing,
cap input and audio sizes, verify CRC-32, and never treat embedded bytes as code or markup.

Supported security fixes target the latest release.
