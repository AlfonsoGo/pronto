#!/bin/bash
# Pronto 2 para Mac · crea el certificado de firma PROPIO (autofirmado, gratis) que usa GitHub para
# firmar Pronto.app siempre con la misma identidad. Así macOS recuerda los permisos (Accesibilidad,
# Micrófono…) de una versión a la siguiente. No sustituye a Apple Developer: la app sigue sin notarizar.
#
#   bash scripts/mac/crear-certificado.sh        (vale en el Mac o en Git Bash de Windows)
#
# Deja en ./certificado-pronto/ :
#   - pronto-cert.p12.base64 → pégalo en GitHub como secreto MAC_CERT_P12
#   - contraseña.txt         → pégala como secreto MAC_CERT_PASSWORD
# (GitHub → repositorio → Settings → Secrets and variables → Actions → New repository secret)
# Guarda la carpeta en un sitio seguro y NO la subas al repositorio: con ella cualquiera firmaría como Pronto.
# Si se pierde, se crea otra; la gente tendrá que volver a dar los permisos una vez.
set -eu
DIR="certificado-pronto"
mkdir -p "$DIR"
cd "$DIR"
if [ -f pronto-cert.p12 ]; then echo "Ya existe $DIR/pronto-cert.p12: no se pisa."; exit 1; fi
PASS="$(openssl rand -hex 16)"
cat > cert.cnf <<'EOF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = Pronto (BeltranLabs)
O = BeltranLabs
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
EOF
openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes -keyout clave.pem -out cert.pem -config cert.cnf
# -legacy: el llavero de macOS no lee el cifrado moderno de OpenSSL 3 en los .p12.
LEGACY=""
openssl pkcs12 -help 2>&1 | grep -q -- '-legacy' && LEGACY="-legacy"
openssl pkcs12 -export $LEGACY -inkey clave.pem -in cert.pem -name "Pronto (BeltranLabs)" -out pronto-cert.p12 -passout "pass:$PASS"
base64 < pronto-cert.p12 | tr -d '\n' > pronto-cert.p12.base64
printf '%s' "$PASS" > contraseña.txt
rm -f clave.pem cert.cnf
echo "Hecho. Secretos para GitHub en $(pwd): pronto-cert.p12.base64 (MAC_CERT_P12) y contraseña.txt (MAC_CERT_PASSWORD)."
