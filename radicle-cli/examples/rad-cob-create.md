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
  "peanut butter": 1,
  "jelly": 0,
  "salad": 2
}

We can implement a program that evaluates the contents of `groceries.jsonl`.
Here's one way of doing that with [jq]:

```
$ ./rad-cob-multiset -- groceries.jsonl
{
  "peanut butter": 1,
  "jelly": 0,
  "salad": 2
}
```

Let's now create a COB:

```
$ rad cob create --repo rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji --type com.example.multiset --message "Create grocery shopping multiset" groceries.jsonl
abc
```

[multisets]: https://wikipedia.org/wiki/Multiset
[JSON]: https://tools.ietf.org/html/std90
[JSON Lines]: https://jsonlines.org/