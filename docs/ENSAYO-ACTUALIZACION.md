# Ensayo de actualización — la puerta antes de publicar

Lo que los tests **no** pueden probar: que un usuario de verdad, con Pronto ya instalado, pase a
la versión nueva **por el actualizador de la propia app** y no pierda nada.

Los tests cubren la lógica (esquema, copia, ajustes, atajo) arrancando de un `pronto.db` real de la
0.7.1. Lo que sigue sin cubrir es el instalador, el ayudante de PowerShell que mata la app, y el
disco de una máquina de verdad. Eso se ensaya a mano, **una vez por versión que cambie el esquema**,
y no se publica sin haberlo hecho.

> 🔴 **Antes de nada.** `dev` y `prod` comparten carpeta de datos (`%APPDATA%\Pronto\Pronto`).
> Este ensayo instala y desinstala Pronto varias veces sobre esa carpeta. **Hazlo en una máquina o
> una cuenta de Windows que no sea la de trabajo**, o copia a mano `%APPDATA%\Pronto` entera a un
> sitio seguro antes de empezar y restáurala al terminar.

## A. Desde la 0.7.1

1. Descarga el instalador de la 0.7.1: <https://github.com/AlfonsoGo/pronto/releases/tag/v0.7.1>
   → el `.exe` de la sección **Assets**.
2. Instálala y ábrela. Espera a que baje el modelo de voz.
3. **Úsala de verdad**, que es lo que crea los datos que hay que salvar:
   - dicta 5 o 6 veces, con nombres propios y palabras tuyas;
   - corrige a mano al menos 3 veces el mismo error (hace falta `freq >= 3` para que la corrección
     se active);
   - cambia el Atajo por uno que no sea el de fábrica y ponlo en «mantener pulsado»;
   - cambia el micrófono y el tamaño de la píldora;
   - deja el Formateo **apagado**.
4. Apunta lo que hay ahora, para comparar luego:
   ```
   copy "%APPDATA%\Pronto\Pronto\pronto.db" "%USERPROFILE%\Desktop\antes.db"
   copy "%APPDATA%\Pronto\Pronto\shared_preferences.json" "%USERPROFILE%\Desktop\antes.json"
   ```
5. Publica la versión nueva (o súbela como release de prueba) y **espera al aviso dentro de la
   app**. Pulsa actualizar **desde Pronto**, no ejecutes el instalador a mano: lo que se ensaya es
   el camino que va a recorrer la gente.
6. Cuando Pronto se reabra solo, comprueba **una por una**:
   - [ ] se abre el **panel** con el aviso de migración y la ruta de la copia;
   - [ ] esa carpeta existe, y dentro están `pronto.db` y `shared_preferences.json`;
   - [ ] **no** hay una carpeta `models` dentro de la copia;
   - [ ] el modelo de voz **no se vuelve a descargar** (no aparece la pantalla de descarga);
   - [ ] tu Atajo es el tuyo y sigue funcionando en «mantener pulsado»;
   - [ ] tu micrófono y el tamaño de la píldora son los que dejaste;
   - [ ] el Formateo sigue **apagado**;
   - [ ] tus correcciones y tu vocabulario siguen ahí (dicta la palabra que corregiste y mira que
         sale bien);
   - [ ] el historial de dictados sigue completo.
7. Compara las bases con la de antes:
   ```
   sqlite3 "%APPDATA%\Pronto\Pronto\pronto.db" "SELECT count(*) FROM corrections; SELECT count(*) FROM vocab_terms; SELECT count(*) FROM dictation_log; PRAGMA user_version;"
   sqlite3 "%USERPROFILE%\Desktop\antes.db"    "SELECT count(*) FROM corrections; SELECT count(*) FROM vocab_terms; SELECT count(*) FROM dictation_log; PRAGMA user_version;"
   ```
   Los tres contadores tienen que salir **iguales**. La `user_version` de la nueva tiene que ser
   mayor; la de `antes.db`, la que tuviera.

## B. Desde la 0.6.0

Lo mismo, empezando por <https://github.com/AlfonsoGo/pronto/releases/tag/v0.6.0>. Es la otra
versión con la que puede haber gente ahí fuera.

## C. El camino de vuelta

8. Con la versión nueva ya instalada y migrada, **instala la 0.7.1 encima**.
9. Comprueba que arranca y que no ha perdido datos (los mismos contadores).
10. Restaura la copia previa **encima** de `%APPDATA%\Pronto\Pronto` —copiar y sobrescribir, nunca
    reemplazar la carpeta— y comprueba que vuelves al estado exacto de antes de actualizar.
    🔴 La copia **no lleva `models/`**: si reemplazas la carpeta en vez de copiar encima, te
    quedas sin el modelo de voz y Pronto se baja 640 MB otra vez.
11. Si algo de esto **no** funciona, se dice en las notas de versión **antes** de publicar. Una
    vuelta atrás que no existe se avisa; no se descubre.

## Qué se apunta

El resultado del ensayo va pegado en el ticket (PRNT-20) con la salida real de los `sqlite3` de
arriba. Sin salida pegada, el ensayo no cuenta.
