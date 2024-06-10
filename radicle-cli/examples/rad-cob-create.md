Handle arbitrary COBs.

Let's introduce a new COB type with the name `com.example.multiset` that implements [multisets],
sometimes also called "bags".

For example, consider a simplified shopping list, where every item on the list is associated with an integral quantity:

 - 5 Bananas
 - 3 Zucchini
 - 1 Bar of Chocolate

The COB we implement should allow two actions:

 - The action `+`, which adds an item or, if already present, increases the associated quantity.
 - The action `-`, which decreases the associated quantity of an item, if it is non-zero.

We model actions as objects in [JSON], and a sequence of actions in [JSON Lines].
For example, to create a sequence of actions:

```
$ cat groceries.jsonl
{"+":"jelly"}
{"+":"peanut butter"}
{"-":"jelly"}
{"-":"jelly"}
{"+":"salad"}
{"+":"salad"}
```

The expected result after evaluating all actions is:

 - 0 Jelly (this could be omitted)
 - 1 Peanut Butter
 - 2 Salad

Or, in JSON:

{
  "jelly": 0,
  "peanut butter": 1,
  "salad": 2
}

We can implement a program that evaluates the contents of `groceries.jsonl`.
Here's one way of doing that with [jq]:

Let's now create a COB:

```
$ rad cob create --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --message "Create grocery shopping multiset" groceries.jsonl
c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1
```

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --object c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1
{
  "jelly": 0,
  "peanut butter": 1,
  "salad": 2
}
```

```
$ rad cob act --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --object c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1 --message "Duplicate groceries" groceries.jsonl
c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1
```

```
$ rad cob log --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --object c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1
commit   32d284202be625e3f90d4eb3f352bdda5f37eea3
parent   c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "+": "jelly"
    }

    {
      "+": "peanut butter"
    }

    {
      "-": "jelly"
    }

    {
      "-": "jelly"
    }

    {
      "+": "salad"
    }

    {
      "+": "salad"
    }

commit   c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1
author   z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi
date     Thu, 15 Dec 2022 17:28:04 +0000

    {
      "+": "jelly"
    }

    {
      "+": "peanut butter"
    }

    {
      "-": "jelly"
    }

    {
      "-": "jelly"
    }

    {
      "+": "salad"
    }

    {
      "+": "salad"
    }

```

```
$ rad cob show --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --object c8e82d31fd8bf6e2e15172e7016f3a6ecdaca9b1
{
  "jelly": 0,
  "peanut butter": 2,
  "salad": 4
}
```

[multisets]: https://wikipedia.org/wiki/Multiset
[JSON]: https://tools.ietf.org/html/std90
[JSON Lines]: https://jsonlines.org/