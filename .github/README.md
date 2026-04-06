# yume

a simple webring

## setup for development

requires: rust, postgresql, [just](https://github.com/casey/just)

```bash
cp .env.example .env

# fill in your values
vim .env

# generate admin password hash
just hash-password 'your-password'
# paste the hash into .env as ADMIN_PASSWORD_HASH

# create database and run migrations
just migrate

# start the server
just dev
```

the app will be available at `http://localhost:3000`.

## deployment

create a `.env` file with your production values:

```bash
ADDR=0.0.0.0:3000
BASE_URL=https://your-webring.example.com
DATABASE_URL=postgres://user:pass@db:5432/webring
ADMIN_PASSWORD_HASH='$argon2id$...'
JWT_SECRET=your-random-secret
JWT_EXPIRY_HOURS=24
TRUST_PROXY=true
```

then build and run:

```bash
docker build -t yume .
docker run -d --name yume --env-file .env -p 3000:3000 yume
```

or with docker compose — bring your own `docker-compose.yml`.

## useful commands

```bash
just              # list all commands
just check        # clippy + fmt check
just fmt          # format code
just test         # run tests
just db-reset     # drop, recreate, migrate
```

## cat in a readme

![cat](https://cataas.com/cat)