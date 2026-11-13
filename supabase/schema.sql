-- GameLib accounts, friends and shared libraries.
--
-- Run this once in your Supabase project: Dashboard → SQL Editor → New
-- query → paste this whole file → Run. Running it again is safe.
--
-- What it sets up:
--   profiles        one row per account: a unique username and display name
--   friendships     friend requests, and accepted friends
--   library_games   each account's game library (as shared by GameLib)
--   user_stats      each account's headline stats
-- Row Level Security keeps it private: anyone signed in can look up a
-- username (to send a friend request), but a library and stats can only
-- be read by their owner and accepted friends, and only the owner can
-- change them.

create extension if not exists citext;

-- Profiles ---------------------------------------------------------------

create table if not exists public.profiles (
  id uuid primary key references auth.users (id) on delete cascade,
  username citext unique not null
    check (char_length(username) between 3 and 24 and username ~ '^[A-Za-z0-9_]+$'),
  display_name text not null default '',
  bio text not null default '',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

alter table public.profiles enable row level security;

drop policy if exists "profiles are visible to signed-in users" on public.profiles;
create policy "profiles are visible to signed-in users" on public.profiles
  for select to authenticated using (true);

drop policy if exists "you can edit your own profile" on public.profiles;
create policy "you can edit your own profile" on public.profiles
  for update to authenticated using (id = auth.uid()) with check (id = auth.uid());

-- Make a profile when someone signs up, from the username GameLib sends.
create or replace function public.handle_new_user()
returns trigger
language plpgsql
security definer set search_path = public
as $$
begin
  insert into public.profiles (id, username, display_name)
  values (
    new.id,
    coalesce(new.raw_user_meta_data ->> 'username', 'player_' || substr(new.id::text, 1, 8)),
    coalesce(new.raw_user_meta_data ->> 'display_name', new.raw_user_meta_data ->> 'username', '')
  );
  return new;
end;
$$;

drop trigger if exists on_auth_user_created on auth.users;
create trigger on_auth_user_created
  after insert on auth.users
  for each row execute function public.handle_new_user();

-- Is this username free? (Checked before signing up, so a taken name
-- gives a clear message instead of a failed sign-up.)
create or replace function public.username_available(name text)
returns boolean
language sql
security definer set search_path = public
stable
as $$
  select not exists (select 1 from public.profiles where username = name::citext);
$$;
grant execute on function public.username_available(text) to anon, authenticated;

-- Friendships ------------------------------------------------------------

create table if not exists public.friendships (
  id bigint generated always as identity primary key,
  requester uuid not null references public.profiles (id) on delete cascade,
  addressee uuid not null references public.profiles (id) on delete cascade,
  status text not null default 'pending' check (status in ('pending', 'accepted')),
  created_at timestamptz not null default now(),
  check (requester <> addressee),
  unique (requester, addressee)
);

alter table public.friendships enable row level security;

drop policy if exists "see your own friendships" on public.friendships;
create policy "see your own friendships" on public.friendships
  for select to authenticated using (auth.uid() in (requester, addressee));

-- Friend requests are only sent, answered and removed through the
-- functions below, which check who is asking. (There are no insert or
-- update policies, so nobody can write a friendship row directly.)
drop policy if exists "send friend requests" on public.friendships;
drop policy if exists "accept requests sent to you" on public.friendships;
drop policy if exists "remove friendships" on public.friendships;

-- Send a friend request by username. If they already asked you, this
-- accepts theirs instead. Returns 'sent', 'accepted', 'already',
-- 'not_found' or 'self'.
create or replace function public.send_friend_request(target text)
returns text
language plpgsql
security definer set search_path = public
as $$
declare
  me uuid := auth.uid();
  them uuid;
begin
  if me is null then
    raise exception 'not signed in';
  end if;
  select id into them from public.profiles where username = target::citext;
  if them is null then
    return 'not_found';
  end if;
  if them = me then
    return 'self';
  end if;
  if exists (select 1 from public.friendships
             where (requester = me and addressee = them)
                or (requester = them and addressee = me and status = 'accepted')) then
    return 'already';
  end if;
  update public.friendships set status = 'accepted'
    where requester = them and addressee = me and status = 'pending';
  if found then
    return 'accepted';
  end if;
  insert into public.friendships (requester, addressee) values (me, them);
  return 'sent';
end;
$$;

-- Accept (or turn down) a request someone sent you.
create or replace function public.respond_friend_request(request bigint, accept boolean)
returns void
language plpgsql
security definer set search_path = public
as $$
begin
  if accept then
    update public.friendships set status = 'accepted'
      where id = request and addressee = auth.uid() and status = 'pending';
  else
    delete from public.friendships
      where id = request and addressee = auth.uid() and status = 'pending';
  end if;
end;
$$;

-- Remove a friend, or cancel a request you sent.
create or replace function public.remove_friend(other uuid)
returns void
language sql
security definer set search_path = public
as $$
  delete from public.friendships
  where (requester = auth.uid() and addressee = other)
     or (requester = other and addressee = auth.uid());
$$;

revoke execute on function public.send_friend_request(text) from public, anon;
revoke execute on function public.respond_friend_request(bigint, boolean) from public, anon;
revoke execute on function public.remove_friend(uuid) from public, anon;
grant execute on function public.send_friend_request(text) to authenticated;
grant execute on function public.respond_friend_request(bigint, boolean) to authenticated;
grant execute on function public.remove_friend(uuid) to authenticated;

-- Are these two accounts friends?
create or replace function public.are_friends(a uuid, b uuid)
returns boolean
language sql
security definer set search_path = public
stable
as $$
  select exists (
    select 1 from public.friendships
    where status = 'accepted'
      and ((requester = a and addressee = b) or (requester = b and addressee = a))
  );
$$;

revoke execute on function public.are_friends(uuid, uuid) from public, anon;
grant execute on function public.are_friends(uuid, uuid) to authenticated;

-- Libraries --------------------------------------------------------------

create table if not exists public.library_games (
  user_id uuid not null references public.profiles (id) on delete cascade,
  game_key text not null,                -- e.g. 'steam:570'
  title text not null,
  source text not null,
  playtime_secs bigint not null default 0,
  last_played bigint,                    -- Unix time
  status text,
  rating smallint,
  favourite boolean not null default false,
  genres text[] not null default '{}',
  updated_at timestamptz not null default now(),
  primary key (user_id, game_key)
);

alter table public.library_games enable row level security;

drop policy if exists "owners and friends can see a library" on public.library_games;
create policy "owners and friends can see a library" on public.library_games
  for select to authenticated using (user_id = auth.uid() or public.are_friends(user_id, auth.uid()));

drop policy if exists "owners change their library" on public.library_games;
create policy "owners change their library" on public.library_games
  for all to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid());

-- Stats ------------------------------------------------------------------

create table if not exists public.user_stats (
  user_id uuid primary key references public.profiles (id) on delete cascade,
  total_playtime_secs bigint not null default 0,
  game_count integer not null default 0,
  played_count integer not null default 0,
  finished_count integer not null default 0,
  session_count integer not null default 0,
  achievements_unlocked integer not null default 0,
  top_game text,
  top_game_secs bigint not null default 0,
  last_played_title text,
  last_played_at bigint,
  updated_at timestamptz not null default now()
);

alter table public.user_stats enable row level security;

drop policy if exists "owners and friends can see stats" on public.user_stats;
create policy "owners and friends can see stats" on public.user_stats
  for select to authenticated using (user_id = auth.uid() or public.are_friends(user_id, auth.uid()));

drop policy if exists "owners change their stats" on public.user_stats;
create policy "owners change their stats" on public.user_stats
  for all to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid());
